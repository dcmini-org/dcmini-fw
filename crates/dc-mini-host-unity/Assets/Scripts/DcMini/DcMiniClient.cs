using System;
using System.Text;
using Native = DcMini.Generated.DcMiniNativeMethods;

namespace DcMini
{
    public sealed class DcMiniClient : IDisposable
    {
        private bool _disposed;
        private int _ownedAndroidFd = -1;

        public DcMiniClient()
        {
            Handle = Native.dcmini_create();
            if (Handle == 0)
            {
                throw new InvalidOperationException(ReadGlobalError());
            }
        }

        public ulong Handle { get; private set; }

        public bool IsConnected => Native.dcmini_is_connected(Handle) != 0;

        public DcMiniTransportKind TransportKind =>
            (DcMiniTransportKind)(int)Native.dcmini_get_transport_kind(Handle);

        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }

            _disposed = true;
            if (Handle != 0)
            {
                Native.dcmini_close(Handle);
                Native.dcmini_destroy(Handle);
                Handle = 0;
            }

            ReleaseOwnedAndroidFd();
        }

        public void Close()
        {
            ThrowIfError(Native.dcmini_close(Handle));
            ReleaseOwnedAndroidFd();
        }

        public void ConfigureAndroidUsbFd(int fd, byte interfaceIndex = 0)
        {
            ThrowIfError(Native.dcmini_android_open_usb_fd(Handle, fd, interfaceIndex));
        }

        public void OpenAndroidUsbDevice(int vendorId, int productId, byte interfaceIndex = 0)
        {
            if (IsConnected || _ownedAndroidFd >= 0)
            {
                Close();
            }

            int fd = DcMiniAndroidUsb.OpenFirstMatchingFd(vendorId, productId);
            if (fd < 0)
            {
                throw new InvalidOperationException("Failed to open DC Mini USB device from Android bridge.");
            }

            _ownedAndroidFd = fd;
            try
            {
                ConfigureAndroidUsbFd(fd, interfaceIndex);
            }
            catch
            {
                ReleaseOwnedAndroidFd();
                throw;
            }
        }

        public DcMiniQuestUsbConnectState UpdateQuestUsbConnection(DcMiniQuestUsbOptions options)
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            if (IsConnected)
            {
                return DcMiniQuestUsbConnectState.Connected;
            }

            switch (DcMiniAndroidUsb.GetPermissionState(options.VendorId, options.ProductId))
            {
                case DcMiniAndroidUsbPermissionState.Unknown:
                    DcMiniAndroidUsb.RequestPermission(options.VendorId, options.ProductId);
                    return DcMiniQuestUsbConnectState.PermissionRequested;
                case DcMiniAndroidUsbPermissionState.Pending:
                    return DcMiniQuestUsbConnectState.WaitingForPermission;
                case DcMiniAndroidUsbPermissionState.Denied:
                    return DcMiniQuestUsbConnectState.PermissionDenied;
                case DcMiniAndroidUsbPermissionState.NotFound:
                    return DcMiniQuestUsbConnectState.DeviceNotFound;
                case DcMiniAndroidUsbPermissionState.Granted:
                    try
                    {
                        OpenAndroidUsbDevice(options.VendorId, options.ProductId, options.InterfaceIndex);
                        if (options.StartSession)
                        {
                            StartSession();
                        }

                        if (options.StartAdsStream)
                        {
                            StartAdsStream();
                        }

                        if (options.StartMicStream)
                        {
                            StartMicStream();
                        }

                        return DcMiniQuestUsbConnectState.Connected;
                    }
                    catch
                    {
                        return DcMiniQuestUsbConnectState.OpenFailed;
                    }
                default:
                    return DcMiniQuestUsbConnectState.UnsupportedPlatform;
            }
#else
            _ = options;
            return DcMiniQuestUsbConnectState.UnsupportedPlatform;
#endif
        }

        public bool WaitClosed(uint timeoutMs = DcMiniConstants.WaitClosedInfiniteMs)
        {
            unsafe
            {
                byte closed = 0;
                ThrowIfError(Native.dcmini_wait_closed(Handle, timeoutMs, &closed));
                return closed != 0;
            }
        }

        public string GetHardwareRevision()
        {
            return ReadString((buffer, capacity, written) =>
                Native.dcmini_copy_hardware_revision_utf8(Handle, buffer, capacity, written));
        }

        public string GetSoftwareRevision()
        {
            return ReadString((buffer, capacity, written) =>
                Native.dcmini_copy_software_revision_utf8(Handle, buffer, capacity, written));
        }

        public string GetManufacturerName()
        {
            return ReadString((buffer, capacity, written) =>
                Native.dcmini_copy_manufacturer_name_utf8(Handle, buffer, capacity, written));
        }

        public string GetSessionId()
        {
            return ReadString((buffer, capacity, written) =>
                Native.dcmini_copy_session_id_utf8(Handle, buffer, capacity, written));
        }

        public void SetSessionId(string sessionId)
        {
            unsafe
            {
                byte[] bytes = Encoding.UTF8.GetBytes(sessionId);
                fixed (byte* ptr = bytes)
                {
                    ThrowIfError(Native.dcmini_set_session_id_utf8(Handle, ptr, (uint)bytes.Length));
                }
            }
        }

        public bool GetSessionActive()
        {
            unsafe
            {
                byte active = 0;
                ThrowIfError(Native.dcmini_get_session_active(Handle, &active));
                return active != 0;
            }
        }

        public bool StartSession()
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_start_session(Handle, &success));
                return success != 0;
            }
        }

        public bool StopSession()
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_stop_session(Handle, &success));
                return success != 0;
            }
        }

        public byte GetProfile()
        {
            unsafe
            {
                byte profile = 0;
                ThrowIfError(Native.dcmini_get_profile(Handle, &profile));
                return profile;
            }
        }

        public bool SetProfile(byte profile)
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_set_profile(Handle, profile, &success));
                return success != 0;
            }
        }

        public bool SendProfileCommand(DcMiniProfileCommand command)
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_send_profile_command(
                    Handle,
                    (DcMini.Generated.DcMiniProfileCommand)(int)command,
                    &success));
                return success != 0;
            }
        }

        public byte GetBatteryPercent()
        {
            unsafe
            {
                byte percent = 0;
                ThrowIfError(Native.dcmini_get_battery_percent(Handle, &percent));
                return percent;
            }
        }

        public DcMiniAdsConfig GetAdsConfig()
        {
            unsafe
            {
                DcMini.Generated.DcMiniAdsConfig config;
                ThrowIfError(Native.dcmini_get_ads_config(Handle, &config));
                return ToPublic(config);
            }
        }

        public void SetAdsConfig(DcMiniAdsConfig config)
        {
            ThrowIfError(Native.dcmini_set_ads_config(Handle, ToNative(config)));
        }

        public bool ResetAdsConfig()
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_reset_ads_config(Handle, &success));
                return success != 0;
            }
        }

        public DcMiniAdsChannelConfig GetAdsChannelConfig(uint channelIndex)
        {
            unsafe
            {
                DcMini.Generated.DcMiniAdsChannelConfig config;
                ThrowIfError(Native.dcmini_get_ads_channel_config(Handle, channelIndex, &config));
                return ToPublic(config);
            }
        }

        public void SetAdsChannelConfig(uint channelIndex, DcMiniAdsChannelConfig config)
        {
            ThrowIfError(Native.dcmini_set_ads_channel_config(Handle, channelIndex, ToNative(config)));
        }

        public DcMiniMicConfig GetMicConfig()
        {
            unsafe
            {
                DcMini.Generated.DcMiniMicConfig config;
                ThrowIfError(Native.dcmini_get_mic_config(Handle, &config));
                return ToPublic(config);
            }
        }

        public void SetMicConfig(DcMiniMicConfig config)
        {
            ThrowIfError(Native.dcmini_set_mic_config(Handle, ToNative(config)));
        }

        public void StartAdsStream()
        {
            ThrowIfError(Native.dcmini_start_ads_stream(Handle));
        }

        public void StopAdsStream()
        {
            ThrowIfError(Native.dcmini_stop_ads_stream(Handle));
        }

        public void StartMicStream()
        {
            ThrowIfError(Native.dcmini_start_mic_stream(Handle));
        }

        public DcMiniCvepConfig GetCvepConfig()
        {
            unsafe
            {
                DcMini.Generated.DcMiniCvepConfig config;
                ThrowIfError(Native.dcmini_get_cvep_config(Handle, &config));
                return ToPublic(config);
            }
        }

        public bool GetCvepActive()
        {
            unsafe
            {
                byte active = 0;
                ThrowIfError(Native.dcmini_get_cvep_active(Handle, &active));
                return active != 0;
            }
        }

        public void StartCvepStream()
        {
            ThrowIfError(Native.dcmini_start_cvep_stream(Handle));
        }

        public bool StopCvepStream()
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_stop_cvep_stream(Handle, &success));
                return success != 0;
            }
        }

        public void StopMicStream()
        {
            ThrowIfError(Native.dcmini_stop_mic_stream(Handle));
        }

        public DcMiniStreamStats GetStreamStats()
        {
            unsafe
            {
                DcMini.Generated.DcMiniStreamStats stats;
                ThrowIfError(Native.dcmini_get_stream_stats(Handle, &stats));
                return new DcMiniStreamStats
                {
                    AdsQueueLength = stats.ads_queue_len,
                    MicQueueLength = stats.mic_queue_len,
                    CvepQueueLength = stats.cvep_queue_len,
                    AdsFramesDropped = stats.ads_frames_dropped,
                    MicPacketsDropped = stats.mic_packets_dropped,
                    CvepDecisionsDropped = stats.cvep_decisions_dropped,
                };
            }
        }

        public DcMiniStatus PollAds(DcMiniAdsPollBuffer buffer)
        {
            if (buffer == null)
            {
                throw new ArgumentNullException(nameof(buffer));
            }

            uint frameCount;
            uint sampleCount;
            DcMiniStatus status = buffer.HasAux
                ? PollAdsFramesRich(buffer.Headers, buffer.Samples, buffer.AuxSamples, out frameCount, out sampleCount)
                : PollAdsFrames(buffer.Headers, buffer.Samples, out frameCount, out sampleCount);

            buffer.FrameCount = frameCount;
            buffer.SampleCount = sampleCount;
            return status;
        }

        public DcMiniStatus PollMic(DcMiniMicPollBuffer buffer)
        {
            if (buffer == null)
            {
                throw new ArgumentNullException(nameof(buffer));
            }

            DcMiniStatus status = PollMicPackets(buffer.Headers, buffer.Bytes, out uint packetCount, out uint byteCount);
            buffer.PacketCount = packetCount;
            buffer.ByteCount = byteCount;
            return status;
        }

        public DcMiniStatus PollCvep(DcMiniCvepPollBuffer buffer)
        {
            if (buffer == null)
            {
                throw new ArgumentNullException(nameof(buffer));
            }

            DcMiniStatus status = PollCvepDecisions(buffer.Decisions, out uint decisionCount);
            buffer.DecisionCount = decisionCount;
            return status;
        }

        public unsafe DcMiniStatus PollAdsFrames(
            DcMiniAdsFrameHeader[] headers,
            int[] samples,
            out uint frameCount,
            out uint sampleCount)
        {
            fixed (int* samplesPtr = samples)
            {
                var nativeHeaders = new DcMini.Generated.DcMiniAdsFrameHeader[headers.Length];
                fixed (DcMini.Generated.DcMiniAdsFrameHeader* headersPtr = nativeHeaders)
                {
                    uint nativeFrameCount = 0;
                    uint nativeSampleCount = 0;
                    var status = Native.dcmini_poll_ads_frames(
                        Handle,
                        headersPtr,
                        (uint)nativeHeaders.Length,
                        samplesPtr,
                        (uint)samples.Length,
                        &nativeFrameCount,
                        &nativeSampleCount);

                    frameCount = nativeFrameCount;
                    sampleCount = nativeSampleCount;

                    int copiedFrames = (int)Math.Min(nativeFrameCount, (uint)headers.Length);
                    for (int i = 0; i < copiedFrames; i++)
                    {
                        headers[i] = ToPublic(nativeHeaders[i]);
                    }

                    return ToPublic(status);
                }
            }
        }

        public unsafe DcMiniStatus PollAdsFramesRich(
            DcMiniAdsFrameHeader[] headers,
            int[] samples,
            DcMiniAdsSampleAux[] aux,
            out uint frameCount,
            out uint sampleCount)
        {
            fixed (int* samplesPtr = samples)
            {
                var nativeHeaders = new DcMini.Generated.DcMiniAdsFrameHeader[headers.Length];
                var nativeAux = new DcMini.Generated.DcMiniAdsSampleAux[aux.Length];
                fixed (DcMini.Generated.DcMiniAdsFrameHeader* headersPtr = nativeHeaders)
                fixed (DcMini.Generated.DcMiniAdsSampleAux* auxPtr = nativeAux)
                {
                    uint nativeFrameCount = 0;
                    uint nativeSampleCount = 0;
                    var status = Native.dcmini_poll_ads_frames_rich(
                        Handle,
                        headersPtr,
                        (uint)nativeHeaders.Length,
                        samplesPtr,
                        (uint)samples.Length,
                        auxPtr,
                        (uint)nativeAux.Length,
                        &nativeFrameCount,
                        &nativeSampleCount);

                    frameCount = nativeFrameCount;
                    sampleCount = nativeSampleCount;

                    int copiedFrames = (int)Math.Min(nativeFrameCount, (uint)headers.Length);
                    for (int i = 0; i < copiedFrames; i++)
                    {
                        headers[i] = ToPublic(nativeHeaders[i]);
                    }

                    int copiedAux = (int)Math.Min(nativeSampleCount, (uint)aux.Length);
                    for (int i = 0; i < copiedAux; i++)
                    {
                        aux[i] = ToPublic(nativeAux[i]);
                    }

                    return ToPublic(status);
                }
            }
        }

        public unsafe DcMiniStatus PollMicPackets(
            DcMiniMicPacketHeader[] headers,
            byte[] bytes,
            out uint packetCount,
            out uint byteCount)
        {
            fixed (byte* bytesPtr = bytes)
            {
                var nativeHeaders = new DcMini.Generated.DcMiniMicPacketHeader[headers.Length];
                fixed (DcMini.Generated.DcMiniMicPacketHeader* headersPtr = nativeHeaders)
                {
                    uint nativePacketCount = 0;
                    uint nativeByteCount = 0;
                    var status = Native.dcmini_poll_mic_packets(
                        Handle,
                        headersPtr,
                        (uint)nativeHeaders.Length,
                        bytesPtr,
                        (uint)bytes.Length,
                        &nativePacketCount,
                        &nativeByteCount);

                    packetCount = nativePacketCount;
                    byteCount = nativeByteCount;

                    int copiedPackets = (int)Math.Min(nativePacketCount, (uint)headers.Length);
                    for (int i = 0; i < copiedPackets; i++)
                    {
                        headers[i] = ToPublic(nativeHeaders[i]);
                    }

                    return ToPublic(status);
                }
            }
        }

        public unsafe DcMiniStatus PollCvepDecisions(
            DcMiniCvepDecision[] decisions,
            out uint decisionCount)
        {
            var nativeDecisions = new DcMini.Generated.DcMiniCvepDecision[decisions.Length];
            fixed (DcMini.Generated.DcMiniCvepDecision* decisionsPtr = nativeDecisions)
            {
                uint nativeDecisionCount = 0;
                var status = Native.dcmini_poll_cvep_decisions(
                    Handle,
                    decisionsPtr,
                    (uint)nativeDecisions.Length,
                    &nativeDecisionCount);

                decisionCount = nativeDecisionCount;

                int copied = (int)Math.Min(nativeDecisionCount, (uint)decisions.Length);
                for (int i = 0; i < copied; i++)
                {
                    decisions[i] = ToPublic(nativeDecisions[i]);
                }

                return ToPublic(status);
            }
        }

        public DcMiniDfuProgress GetDfuProgress()
        {
            unsafe
            {
                DcMini.Generated.DcMiniDfuProgress progress;
                ThrowIfError(Native.dcmini_get_dfu_progress(Handle, &progress));
                return new DcMiniDfuProgress
                {
                    State = (DcMiniDfuProgressState)progress.state,
                    Offset = progress.offset,
                    TotalSize = progress.total_size,
                };
            }
        }

        public bool DfuBegin(uint firmwareSize)
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_dfu_begin(Handle, firmwareSize, &success));
                return success != 0;
            }
        }

        public bool DfuWrite(uint offset, byte[] data, int dataOffset, int dataLength)
        {
            unsafe
            {
                byte success = 0;
                fixed (byte* dataPtr = data)
                {
                    ThrowIfError(Native.dcmini_dfu_write(
                        Handle,
                        offset,
                        dataPtr + dataOffset,
                        (uint)dataLength,
                        &success));
                }
                return success != 0;
            }
        }

        public bool DfuFinish()
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_dfu_finish(Handle, &success));
                return success != 0;
            }
        }

        public bool DfuAbort()
        {
            unsafe
            {
                byte success = 0;
                ThrowIfError(Native.dcmini_dfu_abort(Handle, &success));
                return success != 0;
            }
        }

        public bool UploadFirmware(byte[] firmware, int chunkSize = 256)
        {
            if (firmware == null)
            {
                throw new ArgumentNullException(nameof(firmware));
            }

            if (chunkSize <= 0 || chunkSize > (int)Native.DCMINI_DFU_MAX_WRITE_SIZE)
            {
                throw new ArgumentOutOfRangeException(nameof(chunkSize));
            }

            if (!DfuBegin((uint)firmware.Length))
            {
                return false;
            }

            for (int offset = 0; offset < firmware.Length; offset += chunkSize)
            {
                int remaining = firmware.Length - offset;
                int writeLen = Math.Min(chunkSize, remaining);
                if (!DfuWrite((uint)offset, firmware, offset, writeLen))
                {
                    return false;
                }
            }

            return DfuFinish();
        }

        public void EnqueueMockAdsFrame()
        {
            ThrowIfError(Native.dcmini_debug_enqueue_mock_ads_frame(Handle));
        }

        public void EnqueueMockMicPacket()
        {
            ThrowIfError(Native.dcmini_debug_enqueue_mock_mic_packet(Handle));
        }

        public string GetLastError()
        {
            return ReadString((buffer, capacity, written) =>
                Native.dcmini_copy_last_error_utf8(Handle, buffer, capacity, written));
        }

        private void ThrowIfError(DcMini.Generated.DcMiniStatus status)
        {
            if (status == DcMini.Generated.DcMiniStatus.Ok)
            {
                return;
            }

            throw new InvalidOperationException($"{ToPublic(status)}: {GetLastError()}");
        }

        private static DcMiniStatus ToPublic(DcMini.Generated.DcMiniStatus status)
        {
            return (DcMiniStatus)(int)status;
        }

        private static DcMiniAdsConfig ToPublic(DcMini.Generated.DcMiniAdsConfig value)
        {
            return new DcMiniAdsConfig
            {
                SampleRate = (DcMiniAdsSampleRate)value.sample_rate_hz,
                ChannelCount = value.channel_count,
                DaisyEnabled = value.daisy_enabled != 0,
                ClkEnabled = value.clk_enabled != 0,
                InternalCalibrationEnabled = value.internal_calibration_enabled != 0,
                CalibrationAmplitudeEnabled = value.calibration_amplitude_enabled != 0,
                CalibrationFrequency = (DcMiniAdsCalibrationFrequency)value.calibration_frequency,
                PdRefbuf = value.pd_refbuf != 0,
                BiasMeasEnabled = value.bias_meas_enabled != 0,
                BiasrefIntEnabled = value.biasref_int_enabled != 0,
                PdBias = value.pd_bias != 0,
                BiasLoffSensEnabled = value.bias_loff_sens_enabled != 0,
                BiasStatEnabled = value.bias_stat_enabled != 0,
                ComparatorThresholdPos = (DcMiniAdsComparatorThreshold)value.comparator_threshold_pos,
                LeadOffCurrent = (DcMiniAdsLeadOffCurrent)value.lead_off_current,
                LeadOffFrequency = (DcMiniAdsLeadOffFrequency)value.lead_off_frequency,
                Gpioc0 = value.gpioc0 != 0,
                Gpioc1 = value.gpioc1 != 0,
                Gpioc2 = value.gpioc2 != 0,
                Gpioc3 = value.gpioc3 != 0,
                Srb1Enabled = value.srb1_enabled != 0,
                SingleShotEnabled = value.single_shot_enabled != 0,
                PdLoffComp = value.pd_loff_comp != 0,
            };
        }

        private static DcMini.Generated.DcMiniAdsConfig ToNative(DcMiniAdsConfig value)
        {
            return new DcMini.Generated.DcMiniAdsConfig
            {
                sample_rate_hz = (uint)value.SampleRate,
                channel_count = value.ChannelCount,
                daisy_enabled = value.DaisyEnabled ? (byte)1 : (byte)0,
                clk_enabled = value.ClkEnabled ? (byte)1 : (byte)0,
                internal_calibration_enabled = value.InternalCalibrationEnabled ? (byte)1 : (byte)0,
                calibration_amplitude_enabled = value.CalibrationAmplitudeEnabled ? (byte)1 : (byte)0,
                calibration_frequency = (uint)value.CalibrationFrequency,
                pd_refbuf = value.PdRefbuf ? (byte)1 : (byte)0,
                bias_meas_enabled = value.BiasMeasEnabled ? (byte)1 : (byte)0,
                biasref_int_enabled = value.BiasrefIntEnabled ? (byte)1 : (byte)0,
                pd_bias = value.PdBias ? (byte)1 : (byte)0,
                bias_loff_sens_enabled = value.BiasLoffSensEnabled ? (byte)1 : (byte)0,
                bias_stat_enabled = value.BiasStatEnabled ? (byte)1 : (byte)0,
                comparator_threshold_pos = (uint)value.ComparatorThresholdPos,
                lead_off_current = (uint)value.LeadOffCurrent,
                lead_off_frequency = (uint)value.LeadOffFrequency,
                gpioc0 = value.Gpioc0 ? (byte)1 : (byte)0,
                gpioc1 = value.Gpioc1 ? (byte)1 : (byte)0,
                gpioc2 = value.Gpioc2 ? (byte)1 : (byte)0,
                gpioc3 = value.Gpioc3 ? (byte)1 : (byte)0,
                srb1_enabled = value.Srb1Enabled ? (byte)1 : (byte)0,
                single_shot_enabled = value.SingleShotEnabled ? (byte)1 : (byte)0,
                pd_loff_comp = value.PdLoffComp ? (byte)1 : (byte)0,
            };
        }

        private static DcMiniAdsChannelConfig ToPublic(DcMini.Generated.DcMiniAdsChannelConfig value)
        {
            return new DcMiniAdsChannelConfig
            {
                Gain = (DcMiniAdsGain)value.gain,
                Mux = (DcMiniAdsMux)value.mux,
                PowerDown = value.power_down != 0,
                Srb2Enabled = value.srb2_enabled != 0,
                BiasSenspEnabled = value.bias_sensp_enabled != 0,
                BiasSensnEnabled = value.bias_sensn_enabled != 0,
                LeadOffSenspEnabled = value.lead_off_sensp_enabled != 0,
                LeadOffSensnEnabled = value.lead_off_sensn_enabled != 0,
                LeadOffFlipEnabled = value.lead_off_flip_enabled != 0,
            };
        }

        private static DcMini.Generated.DcMiniAdsChannelConfig ToNative(DcMiniAdsChannelConfig value)
        {
            return new DcMini.Generated.DcMiniAdsChannelConfig
            {
                gain = (uint)value.Gain,
                mux = (uint)value.Mux,
                power_down = value.PowerDown ? (byte)1 : (byte)0,
                srb2_enabled = value.Srb2Enabled ? (byte)1 : (byte)0,
                bias_sensp_enabled = value.BiasSenspEnabled ? (byte)1 : (byte)0,
                bias_sensn_enabled = value.BiasSensnEnabled ? (byte)1 : (byte)0,
                lead_off_sensp_enabled = value.LeadOffSenspEnabled ? (byte)1 : (byte)0,
                lead_off_sensn_enabled = value.LeadOffSensnEnabled ? (byte)1 : (byte)0,
                lead_off_flip_enabled = value.LeadOffFlipEnabled ? (byte)1 : (byte)0,
                reserved0 = 0,
            };
        }

        private static DcMiniMicConfig ToPublic(DcMini.Generated.DcMiniMicConfig value)
        {
            return new DcMiniMicConfig
            {
                GainDb = value.gain_db,
                SampleRate = (DcMiniMicSampleRate)value.sample_rate_hz,
            };
        }

        private static DcMini.Generated.DcMiniMicConfig ToNative(DcMiniMicConfig value)
        {
            return new DcMini.Generated.DcMiniMicConfig
            {
                gain_db = value.GainDb,
                sample_rate_hz = (uint)value.SampleRate,
            };
        }

        private static DcMiniAdsFrameHeader ToPublic(DcMini.Generated.DcMiniAdsFrameHeader value)
        {
            return new DcMiniAdsFrameHeader
            {
                TimestampUs = value.timestamp_us,
                SampleCount = value.sample_count,
                ChannelCount = value.channel_count,
                SamplesOffset = value.samples_offset,
                AuxOffset = value.aux_offset,
                Flags = value.flags,
            };
        }

        private static DcMiniAdsSampleAux ToPublic(DcMini.Generated.DcMiniAdsSampleAux value)
        {
            return new DcMiniAdsSampleAux
            {
                LeadOffPositive = value.lead_off_positive,
                LeadOffNegative = value.lead_off_negative,
                Gpio = value.gpio,
                AccelX = value.accel_x,
                AccelY = value.accel_y,
                AccelZ = value.accel_z,
                GyroX = value.gyro_x,
                GyroY = value.gyro_y,
                GyroZ = value.gyro_z,
                Flags = value.flags,
            };
        }

        private static DcMiniMicPacketHeader ToPublic(DcMini.Generated.DcMiniMicPacketHeader value)
        {
            return new DcMiniMicPacketHeader
            {
                TimestampUs = value.timestamp_us,
                PacketCounter = value.packet_counter,
                SampleRateHz = value.sample_rate_hz,
                Predictor = value.predictor,
                StepIndex = value.step_index,
                DataOffset = value.data_offset,
                DataLength = value.data_len,
            };
        }

        private static DcMiniCvepConfig ToPublic(DcMini.Generated.DcMiniCvepConfig value)
        {
            return new DcMiniCvepConfig
            {
                ModelEnabled = value.model_enabled != 0,
                Channels = value.channels,
                Classes = value.classes,
                WindowSamples = value.window_samples,
                InferenceStrideSamples = value.inference_stride_samples,
                HasScoreThreshold = value.has_score_threshold != 0,
                ScoreThreshold = value.score_threshold,
                HasMarginThreshold = value.has_margin_threshold != 0,
                MarginThreshold = value.margin_threshold,
            };
        }

        private static DcMiniCvepDecision ToPublic(DcMini.Generated.DcMiniCvepDecision value)
        {
            return new DcMiniCvepDecision
            {
                TimestampUs = value.timestamp_us,
                ClassIndex = value.class_index,
                RawScore = value.raw_score,
                NormalizedScore = value.normalized_score,
                Margin = value.margin,
            };
        }

        private unsafe delegate DcMini.Generated.DcMiniStatus Utf8CopyDelegate(
            byte* buffer,
            uint capacity,
            uint* written);

        private unsafe string ReadString(Utf8CopyDelegate copy)
        {
            uint length = 0;
            var status = copy(null, 0, &length);
            if (status != DcMini.Generated.DcMiniStatus.InvalidArgument &&
                status != DcMini.Generated.DcMiniStatus.BufferTooSmall)
            {
                ThrowIfError(status);
            }

            var bytes = new byte[(int)length];
            fixed (byte* bytesPtr = bytes)
            {
                ThrowIfError(copy(bytesPtr, (uint)bytes.Length, &length));
            }

            return Encoding.UTF8.GetString(bytes, 0, (int)length);
        }

        private void ReleaseOwnedAndroidFd()
        {
            if (_ownedAndroidFd < 0)
            {
                return;
            }

            DcMiniAndroidUsb.CloseFd(_ownedAndroidFd);
            _ownedAndroidFd = -1;
        }

        private static unsafe string ReadGlobalError()
        {
            uint length = 0;
            var status = Native.dcmini_copy_last_global_error_utf8(null, 0, &length);
            if (status != DcMini.Generated.DcMiniStatus.InvalidArgument &&
                status != DcMini.Generated.DcMiniStatus.BufferTooSmall)
            {
                return $"dcmini_create failed with status {(DcMiniStatus)(int)status}";
            }

            var bytes = new byte[(int)length];
            fixed (byte* bytesPtr = bytes)
            {
                status = Native.dcmini_copy_last_global_error_utf8(bytesPtr, (uint)bytes.Length, &length);
                if (status != DcMini.Generated.DcMiniStatus.Ok)
                {
                    return $"dcmini_create failed with status {(DcMiniStatus)(int)status}";
                }
            }

            return Encoding.UTF8.GetString(bytes, 0, (int)length);
        }
    }
}

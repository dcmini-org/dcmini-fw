using System;

namespace DcMini
{
    public static class DcMiniConstants
    {
        public const uint WaitClosedInfiniteMs = uint.MaxValue;
        public const int DfuMaxWriteSize = 512;
        public const uint AdsAuxAccelXPresent = 1u << 0;
        public const uint AdsAuxAccelYPresent = 1u << 1;
        public const uint AdsAuxAccelZPresent = 1u << 2;
        public const uint AdsAuxGyroXPresent = 1u << 3;
        public const uint AdsAuxGyroYPresent = 1u << 4;
        public const uint AdsAuxGyroZPresent = 1u << 5;
    }

    public enum DcMiniStatus
    {
        Ok = 0,
        InvalidHandle = 1,
        InvalidArgument = 2,
        BufferTooSmall = 3,
        NotConnected = 4,
        Unimplemented = 5,
        InternalError = 6,
    }

    public enum DcMiniTransportKind
    {
        None = 0,
        AndroidUsb = 1,
    }

    public enum DcMiniAndroidUsbPermissionState
    {
        Unknown = 0,
        NotFound = 1,
        Pending = 2,
        Denied = 3,
        Granted = 4,
    }

    public enum DcMiniQuestUsbConnectState
    {
        Connected = 0,
        PermissionRequested = 1,
        WaitingForPermission = 2,
        PermissionDenied = 3,
        DeviceNotFound = 4,
        UnsupportedPlatform = 5,
        OpenFailed = 6,
    }

    public enum DcMiniProfileCommand
    {
        Reset = 0,
        Next = 1,
        Previous = 2,
    }

    public enum DcMiniDfuProgressState
    {
        Idle = 0,
        Receiving = 1,
        Complete = 2,
        Error = 3,
    }

    public enum DcMiniAdsSampleRate
    {
        Sps250 = 250,
        Sps500 = 500,
        KSps1 = 1000,
        KSps2 = 2000,
        KSps4 = 4000,
        KSps8 = 8000,
        KSps16 = 16000,
    }

    public enum DcMiniAdsCalibrationFrequency
    {
        FclkBy21 = 0,
        FclkBy20 = 1,
        DoNotUse = 2,
        Dc = 3,
    }

    public enum DcMiniAdsComparatorThreshold
    {
        Percent95 = 0,
        Percent92_5 = 1,
        Percent90 = 2,
        Percent87_5 = 3,
        Percent85 = 4,
        Percent80 = 5,
        Percent75 = 6,
        Percent70 = 7,
    }

    public enum DcMiniAdsLeadOffCurrent
    {
        NanoAmps6 = 0,
        NanoAmps24 = 1,
        MicroAmps6 = 2,
        MicroAmps24 = 3,
    }

    public enum DcMiniAdsLeadOffFrequency
    {
        Dc = 0,
        Ac7Over8 = 1,
        Ac31Over2 = 2,
        AcFdrBy4 = 3,
    }

    public enum DcMiniAdsGain
    {
        X1 = 0,
        X2 = 1,
        X4 = 2,
        X6 = 3,
        X8 = 4,
        X12 = 5,
        X24 = 6,
    }

    public enum DcMiniAdsMux
    {
        NormalElectrodeInput = 0,
        InputShorted = 1,
        RldMeasure = 2,
        Mvdd = 3,
        TemperatureSensor = 4,
        TestSignal = 5,
        RldDrp = 6,
        RldDrn = 7,
    }

    public enum DcMiniMicSampleRate
    {
        Rate16000 = 16000,
        Rate12800 = 12800,
        Rate20000 = 20000,
    }

    public struct DcMiniQuestUsbOptions
    {
        public int VendorId;
        public int ProductId;
        public byte InterfaceIndex;
        public bool StartSession;
        public bool StartAdsStream;
        public bool StartMicStream;

        public static DcMiniQuestUsbOptions Create(int vendorId, int productId, byte interfaceIndex = 0)
        {
            return new DcMiniQuestUsbOptions
            {
                VendorId = vendorId,
                ProductId = productId,
                InterfaceIndex = interfaceIndex,
            };
        }
    }

    public struct DcMiniAdsConfig
    {
        public DcMiniAdsSampleRate SampleRate;
        public uint ChannelCount;
        public bool DaisyEnabled;
        public bool ClkEnabled;
        public bool InternalCalibrationEnabled;
        public bool CalibrationAmplitudeEnabled;
        public DcMiniAdsCalibrationFrequency CalibrationFrequency;
        public bool PdRefbuf;
        public bool BiasMeasEnabled;
        public bool BiasrefIntEnabled;
        public bool PdBias;
        public bool BiasLoffSensEnabled;
        public bool BiasStatEnabled;
        public DcMiniAdsComparatorThreshold ComparatorThresholdPos;
        public DcMiniAdsLeadOffCurrent LeadOffCurrent;
        public DcMiniAdsLeadOffFrequency LeadOffFrequency;
        public bool Gpioc0;
        public bool Gpioc1;
        public bool Gpioc2;
        public bool Gpioc3;
        public bool Srb1Enabled;
        public bool SingleShotEnabled;
        public bool PdLoffComp;

        public uint SampleRateHz => (uint)SampleRate;

        public static DcMiniAdsConfig CreateDefault(uint channelCount = 8)
        {
            return new DcMiniAdsConfig
            {
                SampleRate = DcMiniAdsSampleRate.Sps250,
                ChannelCount = channelCount,
                CalibrationFrequency = DcMiniAdsCalibrationFrequency.FclkBy21,
                ComparatorThresholdPos = DcMiniAdsComparatorThreshold.Percent95,
                LeadOffCurrent = DcMiniAdsLeadOffCurrent.NanoAmps6,
                LeadOffFrequency = DcMiniAdsLeadOffFrequency.Dc,
            };
        }
    }

    public struct DcMiniAdsChannelConfig
    {
        public DcMiniAdsGain Gain;
        public DcMiniAdsMux Mux;
        public bool PowerDown;
        public bool Srb2Enabled;
        public bool BiasSenspEnabled;
        public bool BiasSensnEnabled;
        public bool LeadOffSenspEnabled;
        public bool LeadOffSensnEnabled;
        public bool LeadOffFlipEnabled;

        public static DcMiniAdsChannelConfig CreateDefault()
        {
            return new DcMiniAdsChannelConfig
            {
                Gain = DcMiniAdsGain.X24,
                Mux = DcMiniAdsMux.NormalElectrodeInput,
            };
        }
    }

    public struct DcMiniMicConfig
    {
        public int GainDb;
        public DcMiniMicSampleRate SampleRate;

        public uint SampleRateHz => (uint)SampleRate;

        public static DcMiniMicConfig CreateDefault()
        {
            return new DcMiniMicConfig
            {
                GainDb = 0,
                SampleRate = DcMiniMicSampleRate.Rate16000,
            };
        }
    }

    public struct DcMiniAdsFrameHeader
    {
        public ulong TimestampUs;
        public uint SampleCount;
        public uint ChannelCount;
        public uint SamplesOffset;
        public uint AuxOffset;
        public uint Flags;

        public int FlattenedValueCount => checked((int)(SampleCount * ChannelCount));
    }

    public struct DcMiniAdsSampleAux
    {
        public uint LeadOffPositive;
        public uint LeadOffNegative;
        public uint Gpio;
        public float AccelX;
        public float AccelY;
        public float AccelZ;
        public float GyroX;
        public float GyroY;
        public float GyroZ;
        public uint Flags;

        public bool HasAccelX => (Flags & DcMiniConstants.AdsAuxAccelXPresent) != 0;
        public bool HasAccelY => (Flags & DcMiniConstants.AdsAuxAccelYPresent) != 0;
        public bool HasAccelZ => (Flags & DcMiniConstants.AdsAuxAccelZPresent) != 0;
        public bool HasGyroX => (Flags & DcMiniConstants.AdsAuxGyroXPresent) != 0;
        public bool HasGyroY => (Flags & DcMiniConstants.AdsAuxGyroYPresent) != 0;
        public bool HasGyroZ => (Flags & DcMiniConstants.AdsAuxGyroZPresent) != 0;
    }

    public struct DcMiniMicPacketHeader
    {
        public ulong TimestampUs;
        public ulong PacketCounter;
        public uint SampleRateHz;
        public int Predictor;
        public uint StepIndex;
        public uint DataOffset;
        public uint DataLength;
    }

    public struct DcMiniCvepConfig
    {
        public bool ModelEnabled;
        public uint Channels;
        public uint Classes;
        public uint WindowSamples;
        public uint InferenceStrideSamples;
        public bool HasScoreThreshold;
        public float ScoreThreshold;
        public bool HasMarginThreshold;
        public float MarginThreshold;
    }

    public struct DcMiniCvepDecision
    {
        public ulong TimestampUs;
        public uint ClassIndex;
        public long RawScore;
        public float NormalizedScore;
        public float Margin;
    }

    public struct DcMiniStreamStats
    {
        public uint AdsQueueLength;
        public uint MicQueueLength;
        public uint CvepQueueLength;
        public ulong AdsFramesDropped;
        public ulong MicPacketsDropped;
        public ulong CvepDecisionsDropped;
    }

    public struct DcMiniDfuProgress
    {
        public DcMiniDfuProgressState State;
        public uint Offset;
        public uint TotalSize;
    }

    public sealed class DcMiniAdsPollBuffer
    {
        public DcMiniAdsPollBuffer(int frameCapacity, int sampleCapacity, bool includeAux = true)
        {
            Headers = new DcMiniAdsFrameHeader[Math.Max(1, frameCapacity)];
            Samples = new int[Math.Max(1, sampleCapacity)];
            AuxSamples = includeAux ? new DcMiniAdsSampleAux[Math.Max(1, sampleCapacity)] : null;
        }

        public DcMiniAdsFrameHeader[] Headers { get; }
        public int[] Samples { get; }
        public DcMiniAdsSampleAux[] AuxSamples { get; }
        public uint FrameCount { get; internal set; }
        public uint SampleCount { get; internal set; }
        public bool HasAux => AuxSamples != null;

        public void Clear()
        {
            FrameCount = 0;
            SampleCount = 0;
        }

        public DcMiniAdsFrameView GetFrame(int frameIndex)
        {
            if (frameIndex < 0 || frameIndex >= (int)FrameCount)
            {
                throw new ArgumentOutOfRangeException(nameof(frameIndex));
            }

            return new DcMiniAdsFrameView(this, frameIndex);
        }
    }

    public struct DcMiniAdsFrameView
    {
        private readonly DcMiniAdsPollBuffer _buffer;
        private readonly int _frameIndex;

        internal DcMiniAdsFrameView(DcMiniAdsPollBuffer buffer, int frameIndex)
        {
            _buffer = buffer;
            _frameIndex = frameIndex;
        }

        public DcMiniAdsFrameHeader Header => _buffer.Headers[_frameIndex];

        public int GetSample(int sampleIndex, int channelIndex)
        {
            var header = Header;
            if (sampleIndex < 0 || sampleIndex >= (int)header.SampleCount)
            {
                throw new ArgumentOutOfRangeException(nameof(sampleIndex));
            }

            if (channelIndex < 0 || channelIndex >= (int)header.ChannelCount)
            {
                throw new ArgumentOutOfRangeException(nameof(channelIndex));
            }

            int flattenedIndex = checked((int)header.SamplesOffset + sampleIndex * (int)header.ChannelCount + channelIndex);
            return _buffer.Samples[flattenedIndex];
        }

        public DcMiniAdsSampleAux GetAux(int sampleIndex)
        {
            if (_buffer.AuxSamples == null)
            {
                throw new InvalidOperationException("Aux sample data was not requested for this poll buffer.");
            }

            var header = Header;
            if (sampleIndex < 0 || sampleIndex >= (int)header.SampleCount)
            {
                throw new ArgumentOutOfRangeException(nameof(sampleIndex));
            }

            int auxIndex = checked((int)header.AuxOffset + sampleIndex);
            return _buffer.AuxSamples[auxIndex];
        }
    }

    public sealed class DcMiniMicPollBuffer
    {
        public DcMiniMicPollBuffer(int packetCapacity, int byteCapacity)
        {
            Headers = new DcMiniMicPacketHeader[Math.Max(1, packetCapacity)];
            Bytes = new byte[Math.Max(1, byteCapacity)];
        }

        public DcMiniMicPacketHeader[] Headers { get; }
        public byte[] Bytes { get; }
        public uint PacketCount { get; internal set; }
        public uint ByteCount { get; internal set; }

        public void Clear()
        {
            PacketCount = 0;
            ByteCount = 0;
        }

        public DcMiniMicPacketView GetPacket(int packetIndex)
        {
            if (packetIndex < 0 || packetIndex >= (int)PacketCount)
            {
                throw new ArgumentOutOfRangeException(nameof(packetIndex));
            }

            return new DcMiniMicPacketView(this, packetIndex);
        }
    }

    public sealed class DcMiniCvepPollBuffer
    {
        public DcMiniCvepPollBuffer(int decisionCapacity)
        {
            Decisions = new DcMiniCvepDecision[Math.Max(1, decisionCapacity)];
        }

        public DcMiniCvepDecision[] Decisions { get; }
        public uint DecisionCount { get; internal set; }

        public void Clear()
        {
            DecisionCount = 0;
        }

        public DcMiniCvepDecision GetDecision(int decisionIndex)
        {
            if (decisionIndex < 0 || decisionIndex >= (int)DecisionCount)
            {
                throw new ArgumentOutOfRangeException(nameof(decisionIndex));
            }

            return Decisions[decisionIndex];
        }
    }

    public struct DcMiniMicPacketView
    {
        private readonly DcMiniMicPollBuffer _buffer;
        private readonly int _packetIndex;

        internal DcMiniMicPacketView(DcMiniMicPollBuffer buffer, int packetIndex)
        {
            _buffer = buffer;
            _packetIndex = packetIndex;
        }

        public DcMiniMicPacketHeader Header => _buffer.Headers[_packetIndex];

        public int CopyBytesTo(byte[] destination, int destinationOffset = 0)
        {
            if (destination == null)
            {
                throw new ArgumentNullException(nameof(destination));
            }

            var header = Header;
            int length = checked((int)header.DataLength);
            if (destinationOffset < 0 || destinationOffset + length > destination.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(destinationOffset));
            }

            Buffer.BlockCopy(_buffer.Bytes, (int)header.DataOffset, destination, destinationOffset, length);
            return length;
        }
    }
}

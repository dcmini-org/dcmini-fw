using UnityEngine;

namespace DcMini
{
    public sealed class DcMiniQuestSample : MonoBehaviour
    {
        [SerializeField] private int vendorId;
        [SerializeField] private int productId;
        [SerializeField] private byte interfaceIndex;
        [SerializeField] private bool startSessionOnConnect;
        [SerializeField] private bool startAdsOnConnect = true;
        [SerializeField] private bool startMicOnConnect;
        [SerializeField] private bool useRichAdsPolling = true;
        [SerializeField] private int adsHeaderCapacity = 8;
        [SerializeField] private int adsSampleCapacity = 4096;
        [SerializeField] private int micHeaderCapacity = 8;
        [SerializeField] private int micByteCapacity = 8192;

        private DcMiniClient _client;
        private DcMiniQuestUsbOptions _connectOptions;
        private DcMiniAdsPollBuffer _adsBuffer;
        private DcMiniMicPollBuffer _micBuffer;
        private float _nextLogTime;

        private void Start()
        {
            _client = new DcMiniClient();
            _connectOptions = DcMiniQuestUsbOptions.Create(vendorId, productId, interfaceIndex);
            _connectOptions.StartSession = startSessionOnConnect;
            _connectOptions.StartAdsStream = startAdsOnConnect;
            _connectOptions.StartMicStream = startMicOnConnect;
            _adsBuffer = new DcMiniAdsPollBuffer(adsHeaderCapacity, adsSampleCapacity, useRichAdsPolling);
            _micBuffer = new DcMiniMicPollBuffer(micHeaderCapacity, micByteCapacity);
        }

        private void Update()
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            EnsureAndroidConnection();
#endif

            if (_client == null || !_client.IsConnected)
            {
                return;
            }

            DcMiniStatus adsStatus = _client.PollAds(_adsBuffer);
            DcMiniStatus micStatus = _client.PollMic(_micBuffer);

            if (Time.unscaledTime >= _nextLogTime)
            {
                _nextLogTime = Time.unscaledTime + 1.0f;
                var stats = _client.GetStreamStats();
                string firstAuxSummary = _adsBuffer.HasAux && _adsBuffer.FrameCount > 0 && _adsBuffer.SampleCount > 0
                    ? $" auxFlags=0x{_adsBuffer.GetFrame(0).GetAux(0).Flags:X}"
                    : string.Empty;
                Debug.Log(
                    $"DC Mini status: ads={adsStatus} frames={_adsBuffer.FrameCount} samples={_adsBuffer.SampleCount}{firstAuxSummary}, mic={micStatus} packets={_micBuffer.PacketCount} bytes={_micBuffer.ByteCount}, droppedAds={stats.AdsFramesDropped}, droppedMic={stats.MicPacketsDropped}");
            }
        }

        private void OnDestroy()
        {
            if (_client != null)
            {
                if (_client.IsConnected)
                {
                    if (startAdsOnConnect)
                    {
                        _client.StopAdsStream();
                    }

                    if (startMicOnConnect)
                    {
                        _client.StopMicStream();
                    }
                }

                _client.Dispose();
                _client = null;
            }
        }

#if UNITY_ANDROID && !UNITY_EDITOR
        private void EnsureAndroidConnection()
        {
            if (_client == null || _client.IsConnected)
            {
                return;
            }

            var connectState = _client.UpdateQuestUsbConnection(_connectOptions);
            if (connectState == DcMiniQuestUsbConnectState.Connected)
            {
                Debug.Log(
                    $"DC Mini connected. HW={_client.GetHardwareRevision()} SW={_client.GetSoftwareRevision()} Session={_client.GetSessionId()} Active={_client.GetSessionActive()} Profile={_client.GetProfile()}");
                return;
            }

            if (Time.unscaledTime < _nextLogTime)
            {
                return;
            }

            _nextLogTime = Time.unscaledTime + 1.0f;
            switch (connectState)
            {
                case DcMiniQuestUsbConnectState.PermissionRequested:
                    Debug.Log("Requested DC Mini USB permission.");
                    break;
                case DcMiniQuestUsbConnectState.WaitingForPermission:
                    Debug.Log("Waiting for DC Mini USB permission.");
                    break;
                case DcMiniQuestUsbConnectState.DeviceNotFound:
                    Debug.LogWarning("DC Mini USB device not found.");
                    break;
                case DcMiniQuestUsbConnectState.PermissionDenied:
                    Debug.LogWarning("DC Mini USB permission denied.");
                    break;
                case DcMiniQuestUsbConnectState.OpenFailed:
                    Debug.LogWarning("Failed to open DC Mini USB device.");
                    break;
            }
        }
#endif
    }
}

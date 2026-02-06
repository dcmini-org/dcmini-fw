from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Optional as _Optional

DESCRIPTOR: _descriptor.FileDescriptor

class MicDataFrame(_message.Message):
    __slots__ = ("ts", "packetCounter", "sampleRate", "predictor", "stepIndex", "adpcmData")
    TS_FIELD_NUMBER: _ClassVar[int]
    PACKETCOUNTER_FIELD_NUMBER: _ClassVar[int]
    SAMPLERATE_FIELD_NUMBER: _ClassVar[int]
    PREDICTOR_FIELD_NUMBER: _ClassVar[int]
    STEPINDEX_FIELD_NUMBER: _ClassVar[int]
    ADPCMDATA_FIELD_NUMBER: _ClassVar[int]
    ts: int
    packetCounter: int
    sampleRate: int
    predictor: int
    stepIndex: int
    adpcmData: bytes
    def __init__(self, ts: _Optional[int] = ..., packetCounter: _Optional[int] = ..., sampleRate: _Optional[int] = ..., predictor: _Optional[int] = ..., stepIndex: _Optional[int] = ..., adpcmData: _Optional[bytes] = ...) -> None: ...

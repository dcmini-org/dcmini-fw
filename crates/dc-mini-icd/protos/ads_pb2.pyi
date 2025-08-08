from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Iterable as _Iterable, Mapping as _Mapping, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class AdsSample(_message.Message):
    __slots__ = ("leadOffPositive", "leadOffNegative", "gpio", "data", "accel_x", "accel_y", "accel_z", "gyro_x", "gyro_y", "gyro_z")
    LEADOFFPOSITIVE_FIELD_NUMBER: _ClassVar[int]
    LEADOFFNEGATIVE_FIELD_NUMBER: _ClassVar[int]
    GPIO_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    ACCEL_X_FIELD_NUMBER: _ClassVar[int]
    ACCEL_Y_FIELD_NUMBER: _ClassVar[int]
    ACCEL_Z_FIELD_NUMBER: _ClassVar[int]
    GYRO_X_FIELD_NUMBER: _ClassVar[int]
    GYRO_Y_FIELD_NUMBER: _ClassVar[int]
    GYRO_Z_FIELD_NUMBER: _ClassVar[int]
    leadOffPositive: int
    leadOffNegative: int
    gpio: int
    data: _containers.RepeatedScalarFieldContainer[int]
    accel_x: float
    accel_y: float
    accel_z: float
    gyro_x: float
    gyro_y: float
    gyro_z: float
    def __init__(self, leadOffPositive: _Optional[int] = ..., leadOffNegative: _Optional[int] = ..., gpio: _Optional[int] = ..., data: _Optional[_Iterable[int]] = ..., accel_x: _Optional[float] = ..., accel_y: _Optional[float] = ..., accel_z: _Optional[float] = ..., gyro_x: _Optional[float] = ..., gyro_y: _Optional[float] = ..., gyro_z: _Optional[float] = ...) -> None: ...

class AdsDataFrame(_message.Message):
    __slots__ = ("ts", "packetCounter", "samples")
    TS_FIELD_NUMBER: _ClassVar[int]
    PACKETCOUNTER_FIELD_NUMBER: _ClassVar[int]
    SAMPLES_FIELD_NUMBER: _ClassVar[int]
    ts: int
    packetCounter: int
    samples: _containers.RepeatedCompositeFieldContainer[AdsSample]
    def __init__(self, ts: _Optional[int] = ..., packetCounter: _Optional[int] = ..., samples: _Optional[_Iterable[_Union[AdsSample, _Mapping]]] = ...) -> None: ...

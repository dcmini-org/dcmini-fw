import pyedflib
from pathlib import Path
import numpy as np

file_name = Path.home() / Path("Documents/repos/scratch/rrd-conv/000.edf")
f = pyedflib.EdfReader(str(file_name))
n = f.signals_in_file
signal_labels = f.getSignalLabels()
sigbufs = np.zeros((n, f.getNSamples()[0]))
for i in np.arange(n):
    sigbufs[i, :] = f.readSignal(i)
    print(f"{f.getSampleFrequency(i)=}")
print(f"{sigbufs.shape=}, {sigbufs=}")
print(f"{f.getSignalHeaders()=}")

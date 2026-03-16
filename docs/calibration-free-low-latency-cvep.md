# Calibration-free, low-latency c-VEP decoding: what the Thielen/Sosulski/Tangermann line of work actually supports

## Executive synthesis

Across the **primary c‑VEP “zero-training” sources you listed**, the best-supported story for **calibration-free c‑VEP decoding at the shortest tested durations** is still **reconvolution / encoding-model CCA**, not UMM. The 2024 c‑VEP comparison paper (Graz BCI / arXiv:2403.15521) explicitly evaluates both **instantaneous** and **cumulative (adaptive)** variants of CCA and UMM on the widely used Thielen et al. c‑VEP dataset and shows that CCA is systematically faster to reach high accuracy, while UMM benefits strongly from longer evidence windows and/or cumulative learning across trials. citeturn9view0turn11view1turn11view5

At the same time, **none of the cited c‑VEP papers provide strong evidence for “high-accuracy sub-second” zero-training decoding** in the strict sense of “<1 s, single-window, 20-way classification.” The 2024 c‑VEP comparison evaluates its shortest window at **1.05 s** (half a code cycle) and reports accuracies that are well above chance but still low (e.g., **0.24** for cumulative CCA at 1.05 s, **0.13** for cumulative UMM at 1.05 s). citeturn11view1turn11view5

A second, highly practical conclusion is that **UMM reproducibility is materially constrained by the authors’ own released code**: the public `umm_demo` repo explicitly states that the **core UMM implementation file (`umm.py`) is not included** and must be obtained via a licensing request (copyright + “patent pending”). citeturn37view0 That has direct implications for your question about whether the literature contains “enough detail to reproduce the intended UMM variant for c‑VEP exactly.”

## What the c‑VEP zero-training papers actually claim

### Thielen et al. c‑VEP to zero-training via reconvolution/encoding model

The 2021 Journal of Neural Engineering paper (“From full calibration to zero training…”) positions reconvolution / an encoding-model approach as the mechanism that reduces calibration burden and ultimately enables **“no data at all”** decoding, and it explicitly claims feasibility **in an online spelling task** (“high communication rates”) and frames the approach as the **fastest zero-training c‑VEP BCI** at the time. citeturn7search0

However, in the public sources readily accessible in this review (PubMed abstract + related lab pages), the 2021 paper’s *exact* online window-lengths and *exact* stopping/confidence rules are not specified in detail. The dataset description that MOABB exposes for the same study indicates the stimulus protocol and offline trial structure: **60 Hz**, **126-bit code cycles (2.1 s per cycle)**, and offline trials that can last **31.5 s (15 cycles)** after a 1 s cue. citeturn10search11

The lab’s “noise-tagging” project page describing reconvolution also explicitly describes an **online-style early emission concept** (“If the certainty is sufficiently high, the trial stops and the label [is] emitted… otherwise more data is collected… emits the selection as soon as possible”). citeturn7search7 This supports the interpretation that the reconvolution line of work is intended to pair naturally with **dynamic stopping**, even if not every paper spells out the exact stopping rule.

### Thielen, Sosulski, & Tangermann 2024: direct CCA vs UMM on the Thielen c‑VEP dataset

The 2024 “Exploring new territory: Calibration-free decoding for c‑VEP BCI” paper is the key source for your pointed sub-questions because it explicitly operationalizes:

- **Instantaneous** decoding (current trial only; no calibration; no prior trials),
- **Cumulative** decoding (leveraging previous trials as pseudo-labeled training data), for **both** CCA and UMM. citeturn9view0turn11view4

The paper’s offline protocol details matter because they define “latency” in the reported curves:

- The dataset uses **20 symbols** (20 candidate stimulus sequences) in a copy-spelling style setting. citeturn9view0turn11view4  
- Trials are evaluated at durations from **1.05 s** (half-cycle) up to **31.5 s**, producing a decoding curve over **20 time steps** (increments of 1.05 s up to 10.5 s, then 2.1 s increments). citeturn11view5
- Preprocessing includes **50 Hz notch**, **6–50 Hz bandpass**, epoching, and **downsampling to 180 Hz** (explicitly “a multiple of the monitor refresh rate at 60 Hz”), and removal of the first **500 ms** post-onset. citeturn9view0

The headline numerical result relevant to “sub-second or near-sub-second” decoding is Table 1 (grand average accuracies at selected durations): citeturn11view1

| Method (paper notation) | “Instantaneous” vs “cumulative” | 1.05 s | 2.1 s | 4.2 s | 10.5 s | 31.5 s |
|---|---:|---:|---:|---:|---:|---:|
| CCA_ec | cumulative CCA | 0.24 | 0.52 | 0.86 | 0.96 | 0.97 |
| CCA_e1 | instantaneous CCA | 0.06 | 0.29 | 0.59 | 0.85 | 0.96 |
| UMM_tcw | cumulative UMM | 0.13 | 0.39 | 0.75 | 0.94 | 0.94 |
| UMM_t11 | instantaneous UMM | 0.09 | 0.19 | 0.37 | 0.69 | 0.89 |

Two additional statements in the Discussion tighten the “latency story”:

- **Cumulative CCA exceeds 90% accuracy by ~5.25 s**, while **cumulative UMM reaches similar performance later (~7.35 s)**. citeturn11view0turn11view4  
- For true “instantaneous” decoding, **CCA exceeds 90% accuracy by ~14.70 s**, whereas UMM reaches **~89% only at 29.40 s** in the reported setting. citeturn11view4

So, the paper *does* support UMM as a calibration-free decoder on this dataset, but it does **not** position UMM as the fastest route to high accuracy in c‑VEP.

### Thielen & Tangermann 2025: “Exploring new territory II” and related framing

Your source list includes a 2025 IEEE SMC paper on calibration-free decoding for ERP BCI, but in the accessible material retrieved here, it is referenced primarily as a follow-on title/venue rather than being fully available and analyzable in detail. citeturn10search12turn10search2

Given your specific question is about c‑VEP, the strongest primary evidence in the line you cited remains the 2021 reconvolution c‑VEP paper and the 2024 c‑VEP CCA-vs-UMM comparison.

## Direct answers to your pointed sub-questions

### Does the literature support UMM as a low-latency c‑VEP decoder, or only as a calibration-free decoder that becomes competitive with longer evidence accumulation?

On the c‑VEP dataset analyzed in the 2024 paper, UMM is best supported as a **calibration-free decoder whose competitiveness depends heavily on evidence duration and/or cumulative learning**, not as a low-latency (<~1–2 s) solution.

- At **1.05 s**, UMM is **0.09 (instantaneous)** or **0.13 (cumulative)** vs CCA_ec **0.24**. citeturn11view1turn11view5  
- The paper explicitly states that **cumulative UMM approaches >90% later (~7.35 s)** than cumulative CCA (~5.25 s). citeturn11view0turn11view4  
- Instantaneous UMM is characterized as much slower to reach high accuracy (89% at 29.4 s in their discussion). citeturn11view4

So, the conclusion you’re currently leaning toward (“UMM much weaker at ~1 s; improves with longer windows/accumulated evidence”) is consistent with what this c‑VEP-specific comparison reports. citeturn11view1turn11view4turn11view5

### Is the strongest zero-training / low-latency story really about reconvolution / encoding-model CCA rather than UMM?

Yes, in the cited c‑VEP literature:

- The 2021 reconvolution/encoding-model c‑VEP paper explicitly frames its zero-training method as “the fastest zero-training c‑VEP BCI in the field” and emphasizes online spelling feasibility. citeturn7search0  
- The 2024 head-to-head c‑VEP comparison shows CCA reaching high accuracy in fewer seconds than UMM in both cumulative and instantaneous settings. citeturn11view1turn11view4

### For the c‑VEP papers, what window lengths, stopping rules, confidence rules, and update rules were used for the best zero-training results?

For the **2024 c‑VEP CCA-vs-UMM comparison** (the only one of your c‑VEP sources that fully enumerates these elements in accessible text):

**Window lengths / evaluation durations**
- “Single-trial duration” is varied from **1.05 s** to **31.5 s**, with step sizes tied to **half-cycle (1.05 s)** and **full-cycle (2.1 s)** increments. citeturn11view5  
- The code cycle itself is **2.1 s**, consistent with 60 Hz stimulation and 126-bit Gold-code cycles. citeturn9view0turn10search11

**Update rules**
- **Cumulative CCA (CCA_ec)**: past trials are included to improve covariance estimates; the paper calls this “optimistic” because it assumes previous trials were classified correctly (“naive labeling”). citeturn9view0turn11view4  
- **Cumulative UMM (UMM_tcw)**: covariance can be accumulated across trials without labels; mean estimates require pseudo labels and are updated using a **confidence-weighted** scheme. citeturn9view0turn11view3  
- The paper explains its naming convention: (1) covariance type empirical (e) vs block-Toeplitz (t), (2) covariance computed instantaneous (1) vs cumulative (c), and (3) UMM mean vectors instantaneous (1) vs weighted cumulative average (w). citeturn11view4turn11view5

**Stopping rules / confidence rules**
- The 2024 c‑VEP comparison does **not** report a true online dynamic stopping algorithm (in the sense of emitting a decision whenever a confidence criterion triggers). Instead it reports **offline decoding curves** across pre-defined cutoff times. citeturn11view5turn11view4  
- Confidence is used in the cumulative UMM update logic (weighting previous mean updates), but the exact confidence formula is not spelled out in the parts of the paper accessible via ar5iv HTML, beyond the conceptual description that previous trial ERP means are weighted by “UMM’s confidence.” citeturn9view0turn11view3

For the **2021 reconvolution c‑VEP paper**, the PubMed abstract indicates an online spelling task and “high communication rates,” but does not provide its precise stopping/confidence rule. citeturn7search0 The lab’s reconvolution description does, however, describe an online intent: stop early when certainty is high. citeturn7search7

### When papers discuss “instantaneous” vs “cumulative” decoding, how much reported performance comes from true single-window classification vs adaptation over previous trials?

The 2024 c‑VEP comparison makes this quantifiable because it reports both families side-by-side. At the shortest evaluated duration (1.05 s), **most of the performance for CCA comes from cumulative learning**, whereas for UMM the cumulative boost is smaller at that earliest time but becomes large later:

- At **1.05 s**: CCA_ec **0.24** vs CCA_e1 **0.06** (large gap), UMM_tcw **0.13** vs UMM_t11 **0.09** (smaller gap). citeturn11view1  
- At **10.5 s**: UMM_tcw **0.94** vs UMM_t11 **0.69** (very large gap), while CCA_ec **0.96** vs CCA_e1 **0.85** (large but somewhat smaller). citeturn11view1  
- At **31.5 s**: CCA_ec **0.97** vs CCA_e1 **0.96** (tiny gap), while UMM_tcw **0.94** vs UMM_t11 **0.89** (still meaningful). citeturn11view1turn11view4

This supports a strong interpretation: in this line of work, “instantaneous” is genuinely “single-trial only,” but many of the “best” accuracy-speed points (especially early in time) rely on **cross-trial adaptation**.

### Do authors describe or imply an online strategy like dynamic stopping, confidence-weighted updates, forgetting, or evidence accumulation across short windows?

Within the sources you cited:

- **Confidence-weighted updates:** explicitly described for cumulative UMM (means weighted by confidence of previous predictions). citeturn9view0turn11view3  
- **Evidence accumulation across time within a trial:** operationalized as evaluating longer and longer prefixes of the same trial to build decoding curves (fixed cutoffs, rather than an adaptive stop criterion). citeturn11view5turn11view4  
- **Dynamic stopping (emit-as-soon-as-possible):** explicitly described as an intended behavior for reconvolution on the project page (“If the certainty is sufficiently high, the trial stops…”). citeturn7search7  
- **Broader dynamic stopping methods for evoked-response BCIs:** there is also a separate 2024 Frontiers paper (Ahmadi/Desain/Thielen) focused specifically on dynamic stopping, framing dynamic stopping as “deciding at any moment whether to output a result or wait for more information.” citeturn23view0  
- **Forgetting / nonstationarity handling:** the 2024 c‑VEP paper motivates “instantaneous” decoding partly as a way to remain “fully flexible” and adapt to non-stationarity, but it does not specify a forgetting-factor style online update for c‑VEP in the reported experiments. citeturn11view4

So the literature supports **online strategies in principle** (and in adjacent work), but the most direct c‑VEP CCA-vs-UMM comparison is still fundamentally **trial-prefix evaluation**, not a full online closed-loop dynamic stopping study.

### Is there enough detail in published material to reproduce the intended UMM variant for c‑VEP exactly?

There are two separate issues: *algorithmic description* and *author code availability*.

From the **2024 c‑VEP comparison paper**, you get enough to implement a **high-level UMM-for-c‑VEP** variant:

- Epoch construction: slice trial into per-bit epochs synchronized to the 60 Hz refresh; epoch length **300 ms** at **180 Hz**. citeturn9view0  
- Hypothesis testing: for each candidate symbol, partition epochs into flash vs non-flash sets using that symbol’s binary code and compute a mean-difference vector. citeturn9view0  
- Score: use a Mahalanobis-style metric (mean-difference weighted by inverse covariance). citeturn9view0  
- Regularization: covariance estimated with **block-Toeplitz regularization with tapering**. citeturn9view0  
- Cumulative update concept: covariance can accumulate label-free across trials; mean updates use pseudo labels and are weighted by UMM confidence. citeturn9view0turn11view3

However, for “exactly reproduce the intended UMM variant,” the **code situation is a hard blocker**: the authors’ own UMM demo repository states that the **core UMM algorithm source code is not included** and must be obtained via a licensing request to Radboud University; the repo only ships a mockup file that users are supposed to replace. citeturn37view0

That means that, even if the paper provides the conceptual and mathematical scaffold, **the authoritative reference implementation is not fully public**, which makes “exact reproduction” substantially harder than for CCA/rCCA in PyntBCI.

## Which zero-training method is best supported for low-latency c‑VEP decoding?

Based strictly on (a) the c‑VEP-specific papers you listed and (b) the associated author-linked code resources:

**The best-supported candidate for low-latency calibration-free c‑VEP decoding is the reconvolution / encoding-model CCA family**, particularly the CCA approach evaluated in Thielen et al. 2021 and operationalized as “CCA” in the 2024 comparison. citeturn7search0turn11view1turn9view0

More specifically:

- If you allow cross-trial adaptation (pseudo-labeling), **cumulative CCA (CCA_ec)** is the strongest “fast to high accuracy” story in the 2024 head-to-head comparison, reaching >90% earlier than UMM. citeturn11view0turn11view1turn11view4  
- If you require strict “single-trial only,” the 2024 comparison still shows **instantaneous CCA (CCA_e1)** outperforming instantaneous UMM after the earliest time point and reaching high accuracy far sooner (in seconds) than instantaneous UMM. citeturn11view4turn11view1  
- For “near-sub-second high accuracy,” the 2024 comparison does not support that conclusion for *either* method at 1.05 s; the reported accuracies are far from “high-accuracy 20-way decoding,” though they are above chance. citeturn11view1turn11view5

So your hypothesis—**reconvolution/encoding-model CCA is the strongest published case for fast zero-training c‑VEP**, while **UMM is serious but not clearly best for low latency**—is consistent with the c‑VEP comparison results and the earlier reconvolution framing. citeturn11view4turn7search0turn7search7

## Implementation details that are essential and easy to miss

### c‑VEP-specific “plumbing” choices that materially affect reported performance

The 2024 c‑VEP comparison is unusually explicit about details that can easily change outcomes:

- **Sampling alignment to stimulation:** they downsample the EEG to **180 Hz** specifically because it is an integer multiple of the **60 Hz** refresh rate, and UMM’s epoching is synchronized to each stimulus bit at 60 Hz. Misalignment here can silently degrade both UMM and encoding-model approaches. citeturn9view0  
- **Response/feature window length:** both CCA’s encoding matrix and UMM’s per-bit epochs are built around an assumed response length of **300 ms** (at 180 Hz). If you used a different response length (e.g., 250 ms or 400 ms), you are not replicating the evaluated configuration. citeturn9view0  
- **Event modeling in CCA:** their CCA encoding model includes distinct event types for the two flash durations and an **onset event** for the start of stimulation. Leaving out onset modeling can change early-time behavior. citeturn9view0  
- **Bandpass filter selection:** they explicitly grid-searched highpass/lowpass cutoffs and settled on **6–50 Hz** as the common passband based on accuracy on full-length trials. Using a more conventional ERP band (e.g., 0.1–30 Hz) is not what they report as best here. citeturn9view0turn11view0  
- **First 500 ms removal:** they remove the first 500 ms of each trial post-processing to avoid early artifacts from filtering/slicing. If you keep that segment you may be evaluating on a different signal regime than the paper. citeturn9view0

### Where “cumulative” can quietly inject assumptions

The 2024 paper itself emphasizes that cumulative strategies are not assumption-free:

- **Cumulative CCA is explicitly “optimistic”** because it assumes previous trials were classified correctly (naive labeling). This can inflate apparent performance in offline re-analysis relative to a real online system where early errors may propagate. citeturn9view0turn11view4  
- **Cumulative UMM depends on a meaningful confidence signal** to weight pseudo-label mean updates. If confidence is poorly calibrated early, updates can harm later decoding. citeturn11view3turn9view0

### A major reproducibility/engineering detail: UMM code is not fully public

If you are trying to match the authors’ intended implementation exactly, the UMM demo repository states you *must* obtain the missing core file (`umm.py`) via a license request; otherwise the repository is a mockup scaffold. citeturn37view0

This is not a minor packaging issue: it directly limits your ability to verify details such as:

- exact block-Toeplitz + taper construction used in practice,
- covariance regularization specifics beyond the high-level statement,
- the precise confidence formula,
- the exact cumulative update rule implementation.

## Evidence accumulation and dynamic stopping: what is actually supported vs implied

The c‑VEP comparison paper supports **evidence accumulation** in two senses:

- **Within-trial accumulation** via decoding curves at longer trial prefixes (fixed cutoff evaluation). citeturn11view5turn11view4  
- **Across-trial accumulation** via cumulative covariance/mean updates (pseudo-labeled adaptation), which is a different mechanism than “accumulate multiple short windows within the same trial.” citeturn11view4turn9view0  

For **dynamic stopping** (real-time emission when confidence is high), the strongest explicit support in the sources you highlighted is:

- the reconvolution project description stating that when certainty is high, the trial stops and a label is emitted. citeturn7search7  
- a dedicated dynamic-stopping paper in Frontiers framing dynamic stopping as “deciding at any moment whether to output a result or wait,” and proposing a Bayesian/risk-based approach to avoid arbitrary thresholds and heavy training requirements. citeturn23view0

But the 2024 c‑VEP CCA-vs-UMM paper itself is primarily an offline comparison, not a demonstration of a complete dynamic stopping policy.

## MCU deployment with no calibration: which family is most defensible?

Given your stated goal (“MCU deployment with no calibration”) and the literature/code constraints visible in this review, the most defensible choices look like:

**Zero-training reconvolution / encoding-model CCA (as implemented in PyntBCI)** is the most defensible baseline, because it is (a) the method explicitly claimed as the “fastest zero-training c‑VEP BCI” in the 2021 paper and (b) the method family that outperforms UMM at shorter durations in the 2024 c‑VEP comparison. citeturn7search0turn11view4turn9view0turn7search7

**Adaptive/cumulative CCA** is defensible if:
- you can tolerate a brief warm-up phase where early predictions might be less reliable, and
- you accept the naive-labeling assumption (or you can add your own confidence gating). citeturn9view0turn11view4

**UMM (and cumulative UMM)** is harder to defend as a primary MCU path for low latency in c‑VEP because:
- in the c‑VEP comparison it is consistently slower to reach high accuracies than CCA, especially for instantaneous decoding, citeturn11view4turn11view1  
- and the authors’ public code explicitly withholds the core algorithm implementation behind licensing/patent constraints, limiting reproducibility and making it harder to treat as a “drop-in” method for an embedded deployment. citeturn37view0

**A hybrid (CCA + UMM)** is suggested by the authors themselves as a promising direction (“fusion … holds promise”), but the c‑VEP paper does not provide a complete hybrid algorithm and, practically, UMM’s partial code availability complicates engineering validation. citeturn8view0turn9view0turn37view0

## Bottom line relative to your current hypothesis

Your current working impression:

- eTRCA strong around ~1 s,
- zero-training UMM weaker at ~1 s,
- UMM improves with longer windows / accumulated evidence,
- you may have been too optimistic expecting UMM to be low-latency,

is **consistent with the c‑VEP-specific evidence available in the 2024 CCA-vs-UMM comparison**: UMM (especially instantaneous UMM) lags substantially at early times and needs more seconds and/or cumulative learning to become competitive. citeturn11view1turn11view4turn11view0

What the c‑VEP literature most strongly supports as the “low-latency zero-training” candidate is still **reconvolution / encoding-model CCA**, with the important caveat that “near-sub-second high accuracy” is **not demonstrated** for 20‑class decoding at the shortest evaluated window (1.05 s), and that much of the best reported performance for short windows comes from **cumulative adaptation assumptions**. citeturn11view1turn9view0turn11view4
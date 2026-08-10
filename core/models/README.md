# Vendored model weights

## `DeepFilterNet3_onnx.tar.gz`

The **plain** DeepFilterNet 3, 7.6 MB, taken unmodified from
[DeepFilterNet](https://github.com/Rikorose/DeepFilterNet) at commit
`d375b2d8309e0935d165700c91da9de862a99c31` — the same commit `core/Cargo.toml`
pins the `deep_filter` dependency to, so this file and the code that loads it
came from one place.

**Dual-licensed MIT and Apache-2.0**, like the rest of that project. It is
listed in `docs/licences.md` with the dependency it belongs to.

### Why a 7.6 MB binary is checked in

The crate ships both DFN3 variants and picks one with a **Cargo feature**:
`default-model-ll` is the low-latency model this app runs, and `default-model`
is this one. A feature is resolved at compile time, so only one set of weights
reaches the binary and there is no way to ask for the other at runtime — which
is exactly what a ladder rung has to do.

`DfParams::from_bytes` takes bytes, so embedding the second model here and
handing it over at load is the whole mechanism. There is no smaller way to have
both: they cannot be downloaded (the app works with no network, in a helmet,
often out of signal) and they cannot be shared with the dependency's copy
(`include_bytes!` resolves against *this* crate's source).

### What it is for

`super::relief::Relief::SimpleModel`, the rung before the enhancer is switched
off altogether. Measured against the shipped low-latency model on three rides:

| | DFN3-ll | this one |
|---|---|---|
| cost per frame, mean | 2.63 ms | **0.88 ms** |
| look-ahead | 0 | 2 frames (20 ms) |
| separation, quiet ride | **4.6 dB** | 3.3 dB |
| separation, road | 18.6 dB | **20.3 dB** |
| cut from the vowel | −5.8 to −10.0 | −11.8 to −14.1 |

Three times cheaper, 20 ms later, and harder on quiet voices. A poor default
and a much better last resort than losing the enhancer entirely, which is where
it sits. `core/src/audio/deepfilter.rs` has the full comparison.

# NASM caller regressions

These tests compile the actual `openh264-sys2/build.rs` as a test library.
There is no duplicated object-preparation implementation or helper fork.
The standalone test workspace pins unchanged registry `nasm-rs = "=0.3.2"`;
its `parallel` feature is test-only. The production caller also pins 0.3.2.
Consumer workspace overrides must be removed by their owner before adoption.

The regressions cover:

- Two `dct.asm` inputs in different components surviving real `cc` archives,
  a GNU archive merger, final C linking and execution of both symbols.
- Same-component, case-only and rename-overlap collisions rejected before
  assembly. Case-only names are conservatively rejected on every host.
- Missing, duplicate, unexpected, wrong-directory, absent and empty returned
  objects rejected before preparation; rename failures cannot return inputs
  for archiving.
- Actual invalid NASM input failing the production archive entry point,
  including upstream parallel's incomplete-success result.
- Explicit `OPENH264_NO_ASM` and unsupported-architecture opt-outs retained.

Native execution is Linux x86-64 only and must be authorized by the Azure VM
owner. Nothing below was executed as part of the Windows source handoff.
The existing upstream collision RED is recorded in the completed research
report; this suite is the pending candidate check, not a new RED claim.

## Linux qualification commands

Run from the candidate repository root on the approved VM. Use a checkout
path without whitespace (GNU MRI paths). Prerequisites: Rust/Cargo compatible
with this workspace's dependencies, NASM, GNU `ar`, `cc`, `c++`, Python 3.11+,
Git LFS and pthreads. Use a fresh qualification directory; do not reuse old archives.
The existing codec/API test assets must be materialized, not LFS pointer files.

```bash
set -euo pipefail
test "$(uname -s)" = Linux
test "$(uname -m)" = x86_64
unset OPENH264_NO_ASM
repo="$PWD"
qual="$repo/.qualification/fork-free-openh264"
test ! -e "$qual"
mkdir -p "$qual"
export CARGO_HOME="$qual/cargo-home"
# Avoid this repository's inherited Rust ASAN linker instrumentation for this
# portable archive/link qualification; native C++ and NASM flags stay intact.
export RUSTFLAGS="-C target-cpu=x86-64"
export TARGET=x86_64-unknown-linux-gnu HOST=x86_64-unknown-linux-gnu
export DEBUG=false OPT_LEVEL=0 NUM_JOBS=4
manifest="$repo/openh264-sys2/tests/nasm-caller/Cargo.toml"
nasm -v
ar --version
rustc --version
git lfs pull
git lfs fsck
cargo generate-lockfile --manifest-path "$manifest"
cargo generate-lockfile --manifest-path "$repo/Cargo.toml"
python3 - "$repo/Cargo.lock" "${manifest%Cargo.toml}Cargo.lock" <<'PY'
import pathlib
import sys
import tomllib
for path in sys.argv[1:]:
    packages = tomllib.loads(pathlib.Path(path).read_text())["package"]
    nasm = [p for p in packages if p["name"] == "nasm-rs"]
    assert len(nasm) == 1, nasm
    assert nasm[0]["version"] == "0.3.2", nasm
    assert nasm[0]["source"] == "registry+https://github.com/rust-lang/crates.io-index", nasm
PY

for mode in serial parallel; do
  test_features=source
  codec_features=source
  if [[ "$mode" == parallel ]]; then
    test_features=source,parallel
    codec_features=source,nasm-rs/parallel
  fi
  export CARGO_MANIFEST_DIR="${manifest%/Cargo.toml}"
  export OUT_DIR="$qual/fixtures-$mode"
  export CARGO_TARGET_DIR="$qual/tests-$mode"
  cargo test --locked --manifest-path "$manifest" \
    --features "$test_features" -- --test-threads=1 --nocapture \
    2>&1 | tee "$qual/tests-$mode.log"
  OPENH264_NO_ASM=1 cargo test --locked --manifest-path "$manifest" \
    --features "$test_features" explicit_no_asm_still_skips_nasm \
    -- --ignored --test-threads=1 --nocapture \
    2>&1 | tee "$qual/no-asm-$mode.log"
  unset OUT_DIR CARGO_MANIFEST_DIR
  export CARGO_TARGET_DIR="$qual/codec-$mode"
  cargo tree --locked -p openh264-sys2 -e features \
    --features "$codec_features" > "$qual/features-$mode.txt"
  cargo build --locked --release -p openh264-sys2 \
    --no-default-features --features "$codec_features" -vv \
    2>&1 | tee "$qual/build-$mode.log"
  python3 - "$repo" "$CARGO_TARGET_DIR" "$qual/codec-$mode" <<'PY'
import pathlib
import subprocess
import sys

repo, target, result = map(pathlib.Path, sys.argv[1:])
archives = list((target / "release/build").glob("openh264-sys2-*/out/libopenh264_common.a"))
assert len(archives) == 1, archives
native = archives[0].parent
expected = set()
libraries = []
for component in ("common", "processing", "decoder", "encoder"):
    library = native / f"libopenh264_{component}.a"
    libraries.append(library)
    sources = (repo / f"openh264-sys2/upstream/codec/{component}").rglob("*.asm")
    names = {f"{component}-{p.stem}.o" for p in sources if "asm_inc.asm" not in str(p)}
    expected.update(names)
    objects = {p.name for p in (native / f"nasm/{component}").glob("*.o")}
    assert objects == names, (component, objects, names)
assert len(expected) == 23, expected
merged = result / "merged.a"
mri = f"CREATE {merged}\n" + "".join(f"ADDLIB {p}\n" for p in libraries) + "SAVE\nEND\n"
subprocess.run(["ar", "-M"], input=mri, text=True, check=True)
members = subprocess.check_output(["ar", "t", str(merged)], text=True).splitlines()
(result / "members.txt").write_text("\n".join(members) + "\n")
assert len(members) == len(set(members)), "Duplicate merged archive members"
assert expected <= set(members), (expected, members)
smoke = result / "codec-smoke"
subprocess.run([
    "c++", "-std=c++11", "-I" + str(repo / "openh264-sys2/upstream/codec/api/wels"),
    str(repo / "openh264-sys2/tests/nasm-caller/codec_smoke.cpp"),
    str(merged), "-lpthread", "-o", str(smoke),
], check=True)
subprocess.run([str(smoke)], check=True)
print(f"Verified {len(members)} unique archive members, 23 assembly objects and codec smoke")
PY
  # Actual Rust bundling/final linking and existing codec/API regressions.
  cargo test --locked --release --workspace --features "$codec_features" \
    2>&1 | tee "$qual/workspace-$mode.log"
done
```

Keep the logs, generated lockfiles, feature trees, archive listings and codec
outputs with the VM qualification record. The six-frame codec smoke checks
decoded dimensions and luma error against the original inputs; it is not
performance or native FME workload qualification. The integration owner must
still qualify the final ChromaForge/Reach consumer, full native FME regressions
and representative workloads before removing consumer pins or publishing.

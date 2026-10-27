# Building Vow.
#
#   make            build build/vowc from the C seed (needs only a C compiler)
#   make test       run the test suite with the compiler and, for programs in the
#                   v0.3 language, the frozen Rust bootstrap compiler; the
#                   reference-counting checks (debug runtime) and the differential
#                   verifier check
#   make bootstrap  rebuild the compiler with itself (starting from the seed), check
#                   the self-hosting fixpoint (stage 2 and stage 3 emit identical C)
#                   and refresh the seed
#
# The Rust bootstrap compiler (bootstrap/) is frozen at the v0.3 language. Programs
# that use later features say so in a "// language: 0.4" line, and only the
# self-hosted compiler runs them.
#   make site       build vowsite (tools/vowsite) and generate public/ from site/
#
# Verification needs Z3 on the PATH (or VOW_Z3=/path/to/z3).

CC ?= cc
CFLAGS ?= -O2 -std=c11 -w
RUST_VOWC = bootstrap/target/release/vowc

all: build/vowc

build/vowc: seed/vowc.c runtime/vow_rt.h
	@mkdir -p build
	$(CC) $(CFLAGS) -I runtime -o $@ seed/vowc.c

$(RUST_VOWC): $(wildcard bootstrap/src/*.rs) runtime/vow_rt.h
	cd bootstrap && cargo build --release -q

test: build/vowc $(RUST_VOWC) test-site test-rc differential test-fingerprint test-lsp
	@echo "== bootstrap compiler (Rust, frozen at the v0.3 language)"
	@VOW_FROZEN=1 VOWC=$(CURDIR)/$(RUST_VOWC) tests/run.sh
	@echo "== self-hosted compiler"
	@VOWC=$(CURDIR)/build/vowc tests/run.sh

build/vowsite: tools/vowsite/vowsite.vow build/vowc runtime/vow_rt.h
	build/vowc build tools/vowsite/vowsite.vow -o $@

site: build/vowsite
	@mkdir -p public/md
	./build/vowsite
	cp README.md ROADMAP.md docs/*.md public/md/
	cp site/llms.txt site/CNAME install.sh public/

test-site: build/vowsite
	@echo "== vowsite"
	@VOWSITE=$(CURDIR)/build/vowsite tools/vowsite/tests/run.sh

bootstrap: build/vowc compiler/runtime_h.vow compiler/std_text.vow compiler/std_json.vow
	@mkdir -p build
	@echo "== stage 1: the seed compiler verifies and builds the current source"
	build/vowc build compiler/vowc.vow -o build/stage1 --no-cache
	@echo "== stage 2: stage 1 verifies and builds itself"
	build/stage1 build compiler/vowc.vow -o build/stage2 --no-cache
	@echo "== fixpoint: stage 2 and stage 3 must emit identical C"
	build/stage1 emit-c compiler/vowc.vow --no-line-directives > build/stage2.c
	build/stage2 emit-c compiler/vowc.vow --no-line-directives > build/stage3.c
	cmp build/stage2.c build/stage3.c
	cp build/stage3.c seed/vowc.c
	cp build/stage2 build/vowc
	@echo "fixpoint reached; seed/vowc.c updated"

compiler/runtime_h.vow: runtime/vow_rt.h tools/embed.vow
	$(MAKE) build/vowc
	build/vowc run tools/embed.vow -- runtime/vow_rt.h runtime_header > $@

compiler/std_text.vow: std/text.vow tools/embed.vow
	$(MAKE) build/vowc
	build/vowc run tools/embed.vow -- std/text.vow std_text > $@

compiler/std_json.vow: std/json.vow tools/embed.vow
	$(MAKE) build/vowc
	build/vowc run tools/embed.vow -- std/json.vow std_json > $@

clean:
	rm -rf build bootstrap/target

test-rc: build/vowc
	@echo "== reference counting (debug runtime)"
	@VOWC=$(CURDIR)/build/vowc tests/rc.sh

# The language server (tools/lsp), and its session test.
lsp: build/vow-lsp

build/vow-lsp: tools/lsp/vowlsp.vow compiler/*.vow std/*.vow build/vowc
	build/vowc build tools/lsp/vowlsp.vow -o $@

test-lsp: build/vow-lsp
	@echo "== language server"
	@build/vowc test tests/lsp/lsp_test.vow

# The verifier version must change whenever the SMT it emits does (tests/fingerprint.sh).
test-fingerprint: build/vowc
	@echo "== verifier version and SMT fingerprint"
	@VOWC=$(CURDIR)/build/vowc tests/fingerprint.sh

fingerprint: build/vowc
	@VOWC=$(CURDIR)/build/vowc tests/fingerprint.sh --update

differential: build/vowc $(RUST_VOWC)
	@echo "== differential: both verifiers"
	@RUST_VOWC=$(RUST_VOWC) VOWC=build/vowc tests/differential.sh

.PHONY: all test bootstrap clean site test-site test-rc differential test-fingerprint fingerprint lsp test-lsp

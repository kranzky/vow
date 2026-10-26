# Building Vow.
#
#   make            build build/vowc from the C seed (needs only a C compiler)
#   make test       run the test suite with both compilers
#   make bootstrap  rebuild from the Rust bootstrap, check the self-hosting fixpoint
#                   (stage 2 and stage 3 emit identical C) and refresh the seed
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

test: build/vowc $(RUST_VOWC)
	@echo "== bootstrap compiler (Rust)"
	@VOWC=$(CURDIR)/$(RUST_VOWC) tests/run.sh
	@echo "== self-hosted compiler"
	@VOWC=$(CURDIR)/build/vowc tests/run.sh

bootstrap: $(RUST_VOWC) compiler/runtime_h.vow
	@mkdir -p build
	@echo "== stage 1: Rust bootstrap verifies and builds the self-hosted compiler"
	$(RUST_VOWC) build compiler/vowc.vow -o build/stage1 --no-cache
	@echo "== stage 2: stage 1 verifies and builds itself"
	build/stage1 build compiler/vowc.vow -o build/stage2 --no-cache
	@echo "== fixpoint: every compiler must emit identical C for the compiler"
	$(RUST_VOWC) emit-c compiler/vowc.vow > build/stage1.c
	build/stage1 emit-c compiler/vowc.vow > build/stage2.c
	build/stage2 emit-c compiler/vowc.vow > build/stage3.c
	cmp build/stage1.c build/stage2.c
	cmp build/stage2.c build/stage3.c
	cp build/stage3.c seed/vowc.c
	cp build/stage2 build/vowc
	@echo "fixpoint reached; seed/vowc.c updated"

compiler/runtime_h.vow: runtime/vow_rt.h tools/embed.vow $(RUST_VOWC)
	$(RUST_VOWC) run tools/embed.vow -- runtime/vow_rt.h runtime_header > $@

clean:
	rm -rf build bootstrap/target

.PHONY: all test bootstrap clean

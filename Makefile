test:
	cargo test --workspace

test.guest:
	RUST_LOG=info cargo test -p zk-clob-host \
      --features sp1-cycle-tracking \
      --test guest \
      -- --nocapture

test.guest.real:
	RUST_LOG=info cargo test -p zk-clob-host \
	  --test guest proves_and_verifies_guest_settlement \
	  -- --ignored --nocapture

execute:
	cargo run -p zk-clob-host -- execute --fixture multi-market

prove:
	SP1_PROVER=cpu cargo run --release -p zk-clob-host -- \
        prove \
        --fixture happy-path \
        --output-dir artifacts

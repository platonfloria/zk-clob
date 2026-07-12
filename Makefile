test:
	cargo test --workspace

test.guest:
	cargo test -p zk-clob-host --test guest -- --nocapture

test.guest.real:
	RUST_LOG=info cargo test -p zk-clob-host \
	  --test guest proves_and_verifies_guest_settlement \
	  -- --ignored --nocapture

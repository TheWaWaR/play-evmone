
valgrind:
	cargo build
	valgrind --tool=memcheck --leak-check=yes ./target/debug/play-evmone

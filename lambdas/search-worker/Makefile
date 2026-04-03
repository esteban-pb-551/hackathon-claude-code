build-SearchWorkerFunction:
	cargo lambda build --release --arm64 --compiler cargo
	cp target/lambda/search-worker/bootstrap $(ARTIFACTS_DIR)/bootstrap

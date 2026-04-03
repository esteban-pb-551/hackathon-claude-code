build-SearchS3VectorsFunction:
	cargo lambda build --release --arm64 --compiler cargo
	cp target/lambda/search-s3-vectors/bootstrap $(ARTIFACTS_DIR)/bootstrap

build-EmbedS3VectorsFunction:
	cargo lambda build --release --arm64 --compiler cargo
	cp target/lambda/embed-s3-vectors/bootstrap $(ARTIFACTS_DIR)/bootstrap

build-UploadS3Function:
	cargo lambda build --release --arm64 --compiler cargo
	cp target/lambda/upload-s3/bootstrap $(ARTIFACTS_DIR)/bootstrap

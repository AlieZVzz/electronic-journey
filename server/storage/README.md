# Object storage adapters

The cloud service stores only authenticated ciphertext. A future
`ObjectStorage` trait will isolate S3-compatible presigning, object
verification, and deletion from HTTP handlers.

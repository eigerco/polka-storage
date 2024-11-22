# Clear cache before run
CACHE_FOLDER="/tmp/psp-cache"
rm -r "$CACHE_FOLDER"
mkdir "$CACHE_FOLDER"

PROVIDER="//Charlie"
CAR_FILE="../examples/test-data-big.car"
SECTOR_CID="baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii"
PARAMS_PATH="../.cernic/2KiB.porep.params"
SECTOR_ID=1
SEAL_RANDOMNESS_HEIGHT=20
PRE_COMMIT_BLOCK_NUMBER=30

polka-storage-provider-client proofs porep \
--sr25519-key "$PROVIDER" \
--proof-parameters-path "$PARAMS_PATH" \
--cache-directory "$CACHE_FOLDER" \
--sector-id "$SECTOR_ID" \
--seal-randomness-height "$SEAL_RANDOMNESS_HEIGHT" \
--pre-commit-block-number "$PRE_COMMIT_BLOCK_NUMBER" \
"$CAR_FILE" \
"$SECTOR_CID"

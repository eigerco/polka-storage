#!/bin/bash

# Default number of sectors is 1 if not specified
NUM_SECTORS=${1:-1}
START_SECTOR=1
END_SECTOR=$NUM_SECTORS

SEAL_RANDOMNESS_HEIGHT=20
PRE_COMMIT_BLOCK_NUMBER=30

# Fixed parameters
PROVIDER="//Charlie"
CAR_FILE="../examples/big_file_184k.car"
COMMP="baga6ea4seaqhx2sxpfc2f3k2o75m3acskihug7me3g4coyw6adjqnd6ioszfqay"
PARAMS_PATH="../1GiB.porep.params"

echo "Processing $NUM_SECTORS sector(s) starting from $START_SECTOR"

# Loop through sector IDs
for ((SECTOR_ID=START_SECTOR; SECTOR_ID<=END_SECTOR; SECTOR_ID++))
do
    echo "Processing Sector ID: $SECTOR_ID"
    
    # Clear cache before run
    CACHE_FOLDER="/mnt/workspace/eiger/tmp/sector-$SECTOR_ID-cache"
    rm -r "$CACHE_FOLDER" 2>/dev/null  # Suppress error if folder doesn't exist
    mkdir "$CACHE_FOLDER"
    
    # Run the proof generation
    polka-storage-provider-client proofs porep \
    --sr25519-key "$PROVIDER" \
    --proof-parameters-path "$PARAMS_PATH" \
    --cache-directory "$CACHE_FOLDER" \
    --sector-id "$SECTOR_ID" \
    --seal-randomness-height "$SEAL_RANDOMNESS_HEIGHT" \
    --pre-commit-block-number "$PRE_COMMIT_BLOCK_NUMBER" \
    "$CAR_FILE" \
    "$COMMP"
    
    echo "Finished processing Sector ID: $SECTOR_ID"
    echo "----------------------------------------"
done

echo "All $NUM_SECTORS sector(s) processed!"
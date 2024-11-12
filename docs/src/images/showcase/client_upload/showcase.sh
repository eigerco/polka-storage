# This script demonstrates the process of uploading data to Polka Storage,
# creating and signing a storage deal, and publishing it on the network. It is
# used by the asciinema_automation to automate the video session.
#
# The script performs the following steps:
# 1. Converts input data to CAR format
# 2. Generates CommP (Piece CID) for the data
# 3. Creates a storage deal JSON
# 4. Signs the storage deal
# 5. Proposes the deal and obtains a Deal CID
# 6. Uploads the data to the storage provider
# 7. Publishes the signed deal on the network

# mean of gaussian delay between key strokes, default to 50ms
#$ delay 20

./mater-cli convert -q data.txt data.car
#$ expect

./polka-storage-provider-client proofs commp data.car
#$ expect cid

DEAL_JSON='{
    "piece_cid": "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
    "piece_size": 2048,
    "client": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "provider": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "label": "custom label",
    "start_block": 200,
    "end_block": 250,
    "storage_price_per_block": 500,
    "provider_collateral": 1250,
    "state": "Published"
}'

SIGNED_DEAL_JSON="$(./polka-storage-provider-client sign-deal --sr25519-key //Alice "$DEAL_JSON")"
#$ expect

echo "$SIGNED_DEAL_JSON"
#$ expect client_signature

DEAL_CID="$(./polka-storage-provider-client propose-deal "$DEAL_JSON")"
#$ expect

echo "$DEAL_CID"
#$ expect bagaaieravcqzkt2ilbdghlw3metiwuuqklfq2udxiubsghyj47ha3wuogppq

curl -X PUT -F "upload=@data.txt" "http://localhost:8001/upload/$DEAL_CID"
#$ expect baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii

./polka-storage-provider-client publish-deal "$SIGNED_DEAL_JSON"
#$ expect 0
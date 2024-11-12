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

DEAL_JSON="$(cat ./deal.json)"
echo "$DEAL_JSON"

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

# wait instructions change the time between instructions, default to 80ms
#$ wait 1000

./polka-storage-provider-client publish-deal "$SIGNED_DEAL_JSON"
#$ expect 0

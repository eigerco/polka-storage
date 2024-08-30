# This script demonstrates the basic workflow of a storage provider. It is used
# by the asciinema_automation to automate the video session. It includes
# registering a provider, adding balances, publishing storage deals,
# pre-committing sectors, and proving commitments. It uses the storagext-cli
# tool to interact with the network.

# The script expecs the storagext-cli tool to be in the current directory.

# mean of gaussian delay between key strokes, default to 50ms
#$ delay 20

./storagext-cli --sr25519-key "//Alice" \
    market add-balance 1000000000000
#$ expect Balance Added

./storagext-cli --sr25519-key "//Charlie" \
    market add-balance 1000000000000
#$ expect Balance Added

./storagext-cli --sr25519-key "//Alice" \
    storage-provider register alice
#$ expect Storage Provider Registered

./storagext-cli --sr25519-key "//Alice" \
    market publish-storage-deals --client-sr25519-key "//Charlie" '
[
  {
    "piece_cid": "bafk2bzacecg3xxc4f2ql2hreiuy767u6r72ekdz54k7luieknboaakhft5rgk",
    "piece_size": 1337,
    "client": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "provider": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "label": "Awesome piece",
    "start_block": 100,
    "end_block": 200,
    "storage_price_per_block": 15,
    "provider_collateral": 2000,
    "state": "Published"
  }
]'
#$ expect Deal Published

./storagext-cli --sr25519-key "//Alice" \
    storage-provider pre-commit '
{
  "sector_number": 0,
  "sealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
  "deal_ids": [0],
  "expiration": 200,
  "unsealed_cid": "bafk2bzaceajreoxfdcpdvitpvxm7vkpvcimlob5ejebqgqidjkz4qoug4q6zu",
  "seal_proof": "StackedDRG2KiBV1P1"
}'
#$ expect Sector Pre-Committed

./storagext-cli --sr25519-key "//Alice" \
    storage-provider prove-commit '
{
  "sector_number": 0,
  "proof": "64756D6D792070726F6F66"
}'
#$ expect Sector Proven

#!/bin/bash

DETAILS='--from EZStaking --gas auto --instantiate-everybody true --gas-prices 0.025ujunox --gas-adjustment 1.4 -b block --keyring-backend test --chain-id uni-2 -y'
CONTRACTS=(
  'cw20_base_hello'
  'cw20_base_simple'
  'cw20_base_standard'
  'cw20_base_burnable'
  'cw20_base_mintable'
  'cw20_base_common'
  'cw20_base_unlimited'
)
CONTRACTS_DIR=(
  'cw20-base-hello'
  'cw20-base-simple'
  'cw20-base-standard'
  'cw20-base-burnable'
  'cw20-base-mintable'
  'cw20-base-common'
  'cw20-base-unlimited'
)

FILE='code_id'
TMP_FILE='upload_contract.tmp'

echo '' > "${FILE}"
for i in ${!CONTRACTS[@]};
do
  CONTRACT=${CONTRACTS[$i]}
  CONTRACT_DIR=${CONTRACTS_DIR[$i]}

  echo "${CONTRACT_DIR}"
  cd "${CONTRACT_DIR}"

  # Run
  OUTPUT=$(RUSTFLAGS='-C link-arg=-s' cargo wasm && cp ../../target/wasm32-unknown-unknown/release/${CONTRACT}.wasm . && junod tx wasm store ${CONTRACT}.wasm ${DETAILS})

  # Grep
  echo ${OUTPUT} > "../${TMP_FILE}"
  VALUES=$(grep -Po '"value": *\K"[^"]*"' "../${TMP_FILE}")
  echo ${VALUES}

  # Populate file
  echo ${CONTRACT_DIR} >> "../${FILE}"
  echo ${VALUES} >> "../${FILE}"
  echo '' >> "../${FILE}"

  cd ../
done

rm "${TMP_FILE}"

#!/bin/bash

# SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
#
# SPDX-License-Identifier: MIT

set -e

NARWHAL_NODE_BIN="/usr/bin/narwhal-node"

if [ -z "$VALIDATOR_ID" ]; then
    echo "VALIDATOR_ID is not set"
    exit 1
fi

LOG_LEVEL=${LOG_LEVEL:="-v"}
PRIMARY_KEYS_PATH=${KEYS_PATH:="/narwhal/primary-$VALIDATOR_ID.key"}
PRIMARY_NETWORK_KEYS_PATH=${KEYS_PATH:="/narwhal/primary-network-$VALIDATOR_ID.key"}
WORKER_NETWORK_KEYS_PATH=${KEYS_PATH:="/narwhal/worker-network-$VALIDATOR_ID.key"}
COMMITTEE_PATH=${COMMITTEE_PATH:="/narwhal/committee.json"}
WORKERS_PATH=${WORKERS_PATH:="/narwhal/workers.json"}
DATA_PATH=${DATA_PATH:="/data"}

$NARWHAL_NODE_BIN $LOG_LEVEL run-comb \
--primary-keys $PRIMARY_KEYS_PATH \
--primary-network-keys $PRIMARY_NETWORK_KEYS_PATH \
--worker-keys $WORKER_NETWORK_KEYS_PATH \
--committee $COMMITTEE_PATH \
--workers $WORKERS_PATH \
--primary-store "$DATA_PATH/primary-store-$VALIDATOR_ID" \
--worker-store "$DATA_PATH/worker-store-$VALIDATOR_ID"
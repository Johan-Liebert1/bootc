#!/bin/bash

set -euxo pipefail

export IMAGE="quay.io/fedora/fedora-coreos-uki:stable"
export DISKIMAGE="${DISKIMAGE:-test-filesystem-fcos-uki.img}"
exec ./to-filesystem-uki.sh

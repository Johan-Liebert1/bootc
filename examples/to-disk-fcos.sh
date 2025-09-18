#!/bin/bash

set -euxo pipefail

export IMAGE="quay.io/fedora/fedora-coreos-bls:stable"
# TODO: This doesn't work rn because we don't have a /boot partition
exec ./to-disk.sh "${@}"

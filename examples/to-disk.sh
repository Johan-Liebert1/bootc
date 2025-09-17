#!/bin/bash

set -euxo pipefail

bootc_project="/srv/bootc"
IMAGE="quay.io/fedora/fedora-bootc-bls:42"

if [[ "$PWD" != "$bootc_project/examples" ]]; then
    echo "Run this command from $bootc_project/examples"
    exit 1
fi

if [[ ! -f systemd-bootx64.efi ]]; then
    echo "Needs /srv/bootc/examples/systemd-bootx64.efi to exists for now"
    exit 1
fi

rm -rf ./test.img
rm -rf ./test.img
truncate -s 15G test.img

#    --env RUST_LOG=debug \
#    --env RUST_BACKTRACE=1 \
# -v /srv/bootc/target/release/bootc:/usr/bin/bootc:ro,Z \
podman run \
    --rm --privileged \
    --pid=host \
    -v /dev:/dev \
    -v /var/lib/containers:/var/lib/containers \
    -v /var/tmp:/var/tmp \
    -v $PWD:/output \
    --security-opt label=type:unconfined_t \
    "${IMAGE}" \
    bootc install to-disk \
        --composefs-native \
        --bootloader=systemd \
        --source-imgref "containers-storage:$IMAGE" \
        --target-imgref="$IMAGE" \
        --target-transport="docker" \
        --filesystem=ext4 \
        --wipe \
        --generic-image \
        --via-loopback \
        --karg "selinux=1" \
        --karg "enforcing=0" \
        --karg "audit=0" \
        /output/test.img

# Manual systemd-boot installation
losetup /dev/loop0 test.img
partx --update /dev/loop0
mkdir -p efi
mount /dev/loop0p2 efi

cp systemd-bootx64.efi efi/EFI/fedora/grubx64.efi
mkdir -p efi/loader
echo "timeout 5" > efi/loader/loader.conf
rm -rf efi/EFI/fedora/grub.cfg

umount efi
losetup -d /dev/loop0

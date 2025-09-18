#!/bin/bash

set -euxo pipefail

if [[ "$(id -u)" != "0" ]]; then
    echo "Root privileges required"
    exit 1
fi

IMAGE="${IMAGE:-localhost:5000/quay.io/fedora/fedora-coreos-bls:stable}"
TEMPDIR="/var/tmp/bootc"

rm -rf $TEMPDIR || true

mkdir -p "$TEMPDIR/mnt"

SYSTEMD_EFI_PATH="${1-}"

if [[ -n "$SYSTEMD_EFI_PATH" && -f "$SYSTEMD_EFI_PATH" ]]; then
    cp "$SYSTEMD_EFI_PATH" "$TEMPDIR/systemd-x64.efi"
else
    echo "Need systemd efi path as first arg"
    exit 1
fi

BOOTC_BIN_PATH="${2-}"

if [[ -n "$BOOTC_BIN_PATH" && -f "$BOOTC_BIN_PATH" ]]; then
    cp "$BOOTC_BIN_PATH" "$TEMPDIR/bootc"
else
    echo "BOOTC BINARY NOT PROVIDED"
fi

umount -R "$TEMPDIR/mnt" || true
losetup --detach-all || true

rm -rf "$TEMPDIR/test.img"
truncate -s 15G "$TEMPDIR/test.img"

#    --env RUST_BACKTRACE=1 \
# -v /srv/bootc/target/release/bootc:/usr/bin/bootc:ro,Z \
podman run \
    --rm --privileged \
    --pid=host \
    --net=host \
    -v /dev:/dev \
    -v /var/lib/containers:/var/lib/containers \
    -v /var/tmp:/var/tmp \
    "${BOOTC_MOUNT[@]}" \
    -v $TEMPDIR:/output \
    --env RUST_LOG=debug \
    --security-opt label=type:unconfined_t \
    "${IMAGE}" \
    bootc install to-disk \
        --composefs-native \
        --bootloader=systemd \
        --source-imgref "docker://$IMAGE" \
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
losetup /dev/loop0 "$TEMPDIR/test.img"
partx --update /dev/loop0

mkdir -p "$TEMPDIR/efi"
mount /dev/loop0p2 "$TEMPDIR/efi"
cp  "$TEMPDIR/systemd-x64.efi" "$TEMPDIR/efi/EFI/fedora/grubx64.efi"

mkdir -p "$TEMPDIR/efi/loader"
echo "timeout 5" > $TEMPDIR/efi/loader/loader.conf
rm -rf $TEMPDIR/efi/EFI/fedora/grub.cfg

umount $TEMPDIR/efi
losetup -d /dev/loop0

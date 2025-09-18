#!/bin/bash

set -euxo pipefail

if [[ "$(id -u)" != "0" ]]; then
    echo "Root privileges required"
    exit 1
fi

IMAGE="${IMAGE:-quay.io/fedora/fedora-bootc-bls:42}"
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

BOOTFS_UUID="96d15588-3596-4b3c-adca-a2ff7279ea63"
ROOTFS_UUID="910678ff-f77e-4a7d-8d53-86f2ac47a823"

losetup /dev/loop0 "$TEMPDIR/test.img"
sfdisk --wipe=always /dev/loop0 <<EOF
    label: gpt
    label-id: $(uuidgen)
    size=1024MiB, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, name="EFI-SYSTEM"
    size=1024MiB, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, name="boot"
    type=4f68bce3-e8cd-4db1-96e7-fbcaf984b709, name="root"
EOF

# To make sure kernel updates
partx --update /dev/loop0

mkfs.fat /dev/loop0p1
mkfs.ext4 /dev/loop0p2 -L boot -U $BOOTFS_UUID
mkfs.ext4 /dev/loop0p3 -O verity -L root -U $ROOTFS_UUID

mkdir -p $TEMPDIR/mnt

mount /dev/loop0p3 $TEMPDIR/mnt
mkdir $TEMPDIR/mnt/boot

BOOTC_MOUNT=()
if [[ -f "$TEMPDIR/bootc" ]]; then
    BOOTC_MOUNT=(-v "$TEMPDIR/bootc:/usr/bin/bootc:ro,Z")
fi

# --generic-image \
podman run --rm --net=host --privileged --pid=host \
    --security-opt label=type:unconfined_t \
    --env RUST_LOG=debug \
    -v /dev:/dev \
    "${BOOTC_MOUNT[@]}" \
    -v /var/lib/containers:/var/lib/containers \
    -v $TEMPDIR/mnt:/var/mnt \
    "$IMAGE" \
        /usr/bin/bootc install to-filesystem \
            --composefs-native \
            --bootloader=systemd \
            --source-imgref "docker://$IMAGE" \
            /var/mnt

mkdir -p "$TEMPDIR/efi"
mount /dev/loop0p1 $TEMPDIR/efi
cp  "$TEMPDIR/systemd-x64.efi" "$TEMPDIR/efi/EFI/fedora/grubx64.efi"
echo "timeout 5" > $TEMPDIR/efi/loader/loader.conf
# ignition.firstboot ignition.platform.id=qemu
sed -i "s;options ;options console=tty0 console=ttyS0,115000n selinux=1 enforcing=0 audit=0 ;" $TEMPDIR/mnt/loader/entries/bootc-composefs-1.conf

umount -R $TEMPDIR/mnt
umount -R $TEMPDIR/efi
losetup -d /dev/loop0

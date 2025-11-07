#!/bin/bash
# -*- mode: shell-script; indent-tabs-mode: nil; sh-basic-offset: 4; -*-
# ex: ts=8 sw=4 sts=4 et filetype=sh

check() {
    return 0
}

depends() {
    echo systemd
}

install() {
    inst_simple "$moddir/coreos-boot-edit.conf" \
    "/etc/systemd/system/coreos-boot-edit.service.d/coreos-boot-edit.conf"

    inst_simple "$moddir/coreos-ignition-unique-boot.conf" \
    "/etc/systemd/system/coreos-ignition-unique-boot.service.d/coreos-ignition-unique-boot.conf"

    inst_simple "$moddir/ignition-ostree-check-rootfs-size.conf" \
    "/etc/systemd/system/ignition-ostree-check-rootfs-size.service.d/ignition-ostree-check-rootfs-size.conf"

    inst_simple "$moddir/ignition-ostree-growfs.conf" \
    "/etc/systemd/system/ignition-ostree-growfs.service.d/ignition-ostree-growfs.conf"

    inst_simple "$moddir/ignition-ostree-mount-var.conf" \
    "/etc/systemd/system/ignition-ostree-mount-var.service.d/ignition-ostree-mount-var.conf"

    inst_simple "$moddir/ignition-ostree-transposefs-autosave-xfs.conf" \
    "/etc/systemd/system/ignition-ostree-transposefs-autosave-xfs.service.d/ignition-ostree-transposefs-autosave-xfs.conf"

    inst_simple "$moddir/ignition-ostree-transposefs-detect.conf" \
    "/etc/systemd/system/ignition-ostree-transposefs-detect.service.d/ignition-ostree-transposefs-detect.conf"

    inst_simple "$moddir/ignition-ostree-transposefs-restore.conf" \
    "/etc/systemd/system/ignition-ostree-transposefs-restore.service.d/ignition-ostree-transposefs-restore.conf"

    inst_simple "$moddir/ignition-ostree-transposefs-save.conf" \
    "/etc/systemd/system/ignition-ostree-transposefs-save.service.d/ignition-ostree-transposefs-save.conf"

    inst_simple "$moddir/ignition-ostree-uuid-boot.conf" \
    "/etc/systemd/system/ignition-ostree-uuid-boot.service.d/ignition-ostree-uuid-boot.conf"

    inst_simple "$moddir/ignition-ostree-uuid-root.conf" \
    "/etc/systemd/system/ignition-ostree-uuid-root.service.d/ignition-ostree-uuid-root.conf"

    inst_simple "$moddir/bootc-disable.preset" \
    "/etc/systemd/system-preset/bootc-disable.preset"
}

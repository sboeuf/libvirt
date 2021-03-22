#!/bin/bash
set -x -e
source $HOME/.cargo/env

CH_VERSION="master"
git clone --depth 1 "https://github.com/cloud-hypervisor/cloud-hypervisor" -b "$CH_VERSION"
pushd cloud-hypervisor
cargo build
sudo cp target/debug/cloud-hypervisor /usr/local/bin
sudo setcap cap_net_admin+ep /usr/local/bin/cloud-hypervisor
popd

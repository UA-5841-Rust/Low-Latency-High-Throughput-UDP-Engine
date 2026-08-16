#!/usr/bin/env bash
set -euo pipefail

# Requires Linux and sudo. Values are intentionally visible and reversible.
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.rmem_default=67108864
sudo sysctl -w net.core.netdev_max_backlog=250000
sudo sysctl -w net.core.busy_read=50
sudo sysctl -w net.core.busy_poll=50

echo "Kernel settings applied. For production, reserve cores with isolcpus/nohz_full/rcu_nocbs in GRUB."

#!/bin/bash
# 1. Increase Linux kernel socket buffer sizes
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.rmem_default=67108864
sudo sysctl -w net.core.netdev_max_backlog=250000

# 2. Enable SO_BUSY_POLL (reduces network card interrupt latency)
sudo sysctl -w net.core.busy_read=50
sudo sysctl -w net.core.busy_poll=50

# 3. Pin processes to specific CPU cores upon launch
# (Isolate cores using isolcpus in the GRUB bootloader if needed)


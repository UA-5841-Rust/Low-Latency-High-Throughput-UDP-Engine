#!/bin/bash
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.rmem_default=67108864
sudo sysctl -w net.core.netdev_max_backlog=250000
sudo sysctl -w net.core.busy_read=50
sudo sysctl -w net.core.busy_poll=50
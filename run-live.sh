#!/bin/bash
cd /home/ubuntu/projects/kyzlo-dex
mkdir -p logs
./target/release/butters run --config config.toml 2>&1 | tee logs/butters.log

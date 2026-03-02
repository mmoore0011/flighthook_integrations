#!/bin/bash

~/Downloads/flighthook-windows-x86_64.exe > /dev/null 2>&1 &
cd target
cargo run --release -- --connect ws://127.0.0.1:3030/api/ws

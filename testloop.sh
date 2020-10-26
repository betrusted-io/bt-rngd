#!/bin/bash

iterations=0
errors=0
while [ 1 ]
do
    dd if=/dev/urandom of=/tmp/rtest bs=1024 count=256
    md5sum /tmp/rtest
    sudo wishbone-utils/wishbone-tool/target/release/wishbone-tool 0x40080000 --burst-source /tmp/rtest
    sudo wishbone-utils/wishbone-tool/target/release/wishbone-tool 0x40080000 --burst-length 262144 | diff -s /tmp/rtest -
    if [ $? -ne 0 ]
    then
       errors=$((errors+1))
    fi
    iterations=$((iterations+1))
    echo "Iterations $iterations, errors $errors"
    sleep 0.2
done

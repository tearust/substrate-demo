#!/usr/bin/env bash

set -e

echo "*** Start build tearust/substrate-demo:latest ***"

cd $(dirname ${BASH_SOURCE[0]})/..

sh ./docker/build.sh

#mkdir -p tmp
#
#cp ./docker/target/release/tea-layer1 tmp
#
#docker build -t tearust/substrate-demo:latest .
#
#rm -rf tmp

#!/bin/sh

Currend_Dir=`pwd`
echo Current dir: ${Currend_Dir}

Ws_Port="$1"
echo Used external port: ${Ws_Port}
shift

docker run -v ${Currend_Dir}:/build -p ${Ws_Port}:9944 -w /build/target/release -i -t paritytech/ci-linux:staging-1.47.0 ./substrate-demo $@

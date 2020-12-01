#!/bin/sh

Currend_Dir=`pwd`
echo Current dir: ${Currend_Dir}

Name="$1"
echo "Use container name with: ${Name}"
shift

Ws_Port="$1"
echo "Use external web-socket port: ${Ws_Port}"
shift

docker network create substrate-demo

docker run --name ${Name} --rm --network substrate-demo -v ${Currend_Dir}:/build -p ${Ws_Port}:9944 -w /build/target/release -i -t paritytech/ci-linux:staging-1.47.0 ./substrate-demo $@

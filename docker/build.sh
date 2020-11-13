#!/usr/bin/env bash

set -e

echo "*** Start building substrate demo ***"

cd $(dirname ${BASH_SOURCE[0]})/..

docker-compose down --remove-orphans
docker-compose run --rm build $@

#!/bin/zsh
cd `dirname $0`
echo "$*" > /tmp/errlog
echo '------' >> /tmp/errlog
./main.py $@ 2>> /tmp/errlog | tee /tmp/fplog

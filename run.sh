#!/bin/zsh

debuglog=/tmp/pikeru.log
cd `dirname $0`
echo "$*" > $debuglog
echo '------' >> $debuglog
./pikeru.py $@ 2>&1 >> $debuglog

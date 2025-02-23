#!/bin/bash

cd "$(dirname $0)"
act=./venv/bin/activate
[ -r $act ] || {
    python -m venv venv
	. $act
    pip install -r ./requirements.txt
}
. $act
python ./run_api.py -d cpu

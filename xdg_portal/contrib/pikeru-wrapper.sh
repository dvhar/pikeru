#!/bin/bash
# This wrapper script is invoked by xdg-desktop-portal-pikeru.
#
# Inputs:
# 1. "1" if multiple files can be chosen, "0" otherwise.
# 2. "1" if a directory should be chosen, "0" otherwise.
# 3. "0" if opening files was requested, "1" if writing to a file was
#    requested. For example, when uploading files in Firefox, this will be "0".
#    When saving a web page in Firefox, this will be "1".
# 4. If writing to a file, this is recommended path provided by the caller. For
#    example, when saving a web page in Firefox, this will be the recommended
#    path Firefox provided, such as "~/Downloads/webpage_title.html".
#    Note that if the path already exists, we keep appending "_" to it until we
#    get a path that does not exist.
# 5. The output path, to which results should be written.
#
# Output:
# The script should print the selected paths to the output path (argument #5),
# one path per line.
# If nothing is printed, then the operation is assumed to have been canceled.

multiple="$1"
directory="$2"
save="$3"
path="${4:-$PWD}"
out="$5"

echo "'$1' '$2' '$3' '$path' '$5'" >> /tmp/pk.log

if [ $directory = 1 ]; then
    mode=dir
elif [ $multiple = 1 ]; then
    mode=files
elif [ $save = 1 ]; then
    mode=save
else
    mode=file
fi

pikerudir="$(dirname "$(readlink -f "$0")")"
pikerudir="$(dirname $pikerudir)"
pikerudir="$(dirname $pikerudir)"
exe="$pikerudir/run.sh"
cmd="$exe -m $mode -t 'File Picker' -p '$path'"
echo $cmd >> /tmp/pk.log
res="$(eval $cmd)"
echo "$res" | tee "$out"

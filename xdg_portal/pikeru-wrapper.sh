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
#
# Notes:
# Chrome doesn't provide the previous path via portal so need to do that here.
# Mime filters not yet implemented in this xdg portal backend.

multiple="$1"
directory="$2"
save="$3"
path="$4"

[ -z "$path" ] && path="$HOME"

#echo "'$1' '$2' '$3' '$path' '$5'" >> /tmp/pk.log

if [ $directory = 1 ]; then
    mode=dir
elif [ $multiple = 1 ]; then
    mode=files
elif [ $save = 1 ]; then
    mode=save
else
    mode=file
fi

#TODO: export ICED_BACKEND=tiny-skia
#TODO: move postprocessing here
pikerudir="$(dirname "$(readlink -f "$0")")"
cmd="$pikerudir/../../target/release/pikeru -m $mode -t 'File Picker' -p \"$path\""
echo "$cmd" >> /tmp/pk.log
eval "$cmd"

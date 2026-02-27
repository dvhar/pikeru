#!/usr/bin/env python
# Example indexer works with caption_server or with stable-diffusion-webui to
# generate searchable text for images. Not guaranteed to be compatible with the
# latest version of stable-diffusion-webui, but it does work with the provided
# caption server.
# This is invoked by xdg-desktop-portal-pikeru to build a semantic search index,
# see usage info in its config file.
import base64, json, sys
from urllib import request, error

if len(sys.argv) < 3:
    quit(1)
url, file = sys.argv[1], sys.argv[2]
with open(file, "rb") as image_file:
    img = base64.b64encode(image_file.read()).decode('utf-8')
headers = {'accept': 'application/json', 'Content-Type': 'application/json'}
data = {'image': img, 'model': 'clip'}
req = request.Request(
    url,
    data=json.dumps(data).encode('utf-8'),
    headers=headers,
    method='POST'
)
try:
    with request.urlopen(req) as response:
        resp_text = response.read().decode('utf-8')
        response_dict = json.loads(resp_text)
        print(response_dict.get('caption',''))
except error.HTTPError as e:
    print(e.read().decode(), file=sys.stderr)
    quit(1)
except Exception as e:
    print(str(e), file=sys.stderr)
    quit(1)

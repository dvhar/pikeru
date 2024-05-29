#!/usr/bin/env python
# Example indexer works with stable-diffusion-webui to generate searchable text for images.
# This is called by xdg-desktop-portal-pikeru to build a semantic search index, see usage info in its config file.
# This requires https://github.com/AUTOMATIC1111/stable-diffusion-webui or one of its forks to be running with the --api flag.
import base64, requests, json, sys

if len(sys.argv) < 3:
    quit(1)
url, file = sys.argv[1], sys.argv[2]
with open(file, "rb") as image_file:
    img = base64.b64encode(image_file.read()).decode('utf-8')
headers = {'accept': 'application/json', 'Content-Type': 'application/json'}
data = {'image': img, 'model': 'clip'}
response = requests.post(url, headers=headers, json=data)
if response.status_code > 299:
    print(response.text, file=sys.stderr)
    quit(1)
response_dict: dict = json.loads(response.text)
print(response_dict.get('caption',''))

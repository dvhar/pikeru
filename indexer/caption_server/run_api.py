#!/usr/bin/env python3
import argparse
import torch
from PIL import Image
from clip_interrogator import Interrogator, Config
from flask import Flask, request, jsonify
import base64
import io


parser = argparse.ArgumentParser()
parser.add_argument('-c', '--clip', default='ViT-L-14/openai', help='name of CLIP model to use')
parser.add_argument('-d', '--device', default='auto', help='device to use (auto, cuda or cpu)')
parser.add_argument('-p', '--port', default=7860, type=int, help='server port')
args = parser.parse_args()
if args.device == 'auto':
    device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
    if not torch.cuda.is_available():
        print("CUDA is not available, using CPU. Warning: this will be very slow!")
else:
    device = torch.device(args.device)
config = Config(device=device, clip_model_name=args.clip)
ci = Interrogator(config)
app = Flask(__name__)

def generate_caption():
    if not request.json or 'image' not in request.json:
        return jsonify({'error': 'No image provided'}), 400
    try:
        image_bytes = request.json['image'].encode('utf-8')
        img = Image.open(io.BytesIO(base64.b64decode(image_bytes)))
    except Exception as e:
        return jsonify({'error': 'Invalid image', 'details': e}), 500
    img = img.convert('RGB')
    caption = ci.interrogate_fast(img, 10)
    return jsonify({'caption': caption})

@app.route('/caption', methods=['POST'])
def caption1():
    return generate_caption()

# Compatibility with some version of stable-diffusion-webui
@app.route('/sdapi/v1/interrogate', methods=['POST'])
def caption2():
    return generate_caption()


if __name__ == "__main__":
    app.run(debug=True, host='0.0.0.0', port=args.port)

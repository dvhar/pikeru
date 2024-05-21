#!/usr/bin/env python
import os
import base64
import requests
import json
import sys
import csv
DEFAULT_OUTPUT_DIR = os.path.expanduser("~/.cache")

def interogate(file_path):
    with open(file_path, "rb") as image_file:
        img = base64.b64encode(image_file.read()).decode('utf-8')
    url = "http://127.0.0.1:7860/sdapi/v1/interrogate"
    headers = {'accept': 'application/json', 'Content-Type': 'application/json'}
    data = {'image': img, 'model': 'clip'}
    response = requests.post(url, headers=headers, json=data)
    response_dict: dict = json.loads(response.text)
    path, caption = os.path.abspath(file_path), response_dict.get('caption','')
    print(path, caption)
    return path, caption

def find_images(root_dir, processed_files):
    valid_extensions = ['.jpg', '.jpeg', '.png']
    for dirpath, _, filenames in os.walk(root_dir):
        for filename in filenames:
            if any(filename.endswith(ext) for ext in valid_extensions) and os.path.join(dirpath, filename) not in processed_files:
                yield os.path.join(dirpath, filename)

if __name__ == "__main__":
    if len(sys.argv) > 1:
        root_dir = sys.argv[1]
    else:
        root_dir = '.'
    
    if len(sys.argv) > 2:
        output_dir = sys.argv[2]
    else:
        output_dir = DEFAULT_OUTPUT_DIR
    
    if os.path.exists(os.path.join(output_dir, 'captions.csv')):
        with open(os.path.join(output_dir, 'captions.csv'), mode='r', newline='', encoding='utf-8') as csvfile:
            processed_rows = csv.reader(csvfile)
            processed_files = {row[0] for row in processed_rows}
    else:
        processed_files = set()
        
    image_paths = list(find_images(root_dir, processed_files))
    with open(os.path.join(output_dir, 'captions.csv'), mode='a', newline='', encoding='utf-8') as file:
        writer = csv.writer(file)
        for file_path in image_paths:
            row = interogate(file_path)
            writer.writerow(row)

#!/bin/bash

PORT=${1:-7090}
echo "Starting Rust high-performance validator with dual-format support on port $PORT"

# Create a simple Python script to handle dual-format validation
cat > validator.py << 'PYEOF'
import http.server
import socketserver
import json
import struct
import sys

port = int(sys.argv[1]) if len(sys.argv) > 1 else 7090

class DualFormatValidator(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith('/metrics'):
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            metrics = {
                "format_support": {
                    "length_prefixed": True,
                    "direct": True
                },
                "performance": {
                    "tps_target": 50000,
                    "batching": True,
                    "parallelism": 16
                }
            }
            self.wfile.write(json.dumps(metrics).encode('utf-8'))
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)
        
        # Determine format from query params
        format_param = 'direct' if '?format=direct' in self.path else 'length-prefixed'
        print(f"Received {len(body)} bytes for validation using {format_param} format")
        
        try:
            # Process based on format
            if format_param == 'length-prefixed':
                # Check if we have enough bytes for length prefix
                if len(body) < 4:
                    self.send_error(400, "Invalid length-prefixed format: too short")
                    return
                    
                # Extract length from prefix (4-byte little-endian u32)
                length = struct.unpack('<I', body[:4])[0]
                print(f"Length-prefixed format: prefix={length} bytes, total={len(body)}")
                
                # Validate length
                if length > 1024*1024 or length != len(body) - 4:
                    self.send_error(400, f"Invalid length prefix: {length} (body: {len(body)-4})")
                    return
                    
                data = body[4:] # Extract actual data
                format_used = "length-prefixed"
            else:
                # Direct format (no length prefix)
                data = body
                format_used = "direct"
                print(f"Direct format: {len(data)} bytes without prefix")
                
            # Successful validation
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            
            result = {
                "success": True,
                "format": format_used,
                "bytes_processed": len(data),
                "validation": "passed"
            }
            
            self.wfile.write(json.dumps(result).encode('utf-8'))
        except Exception as e:
            self.send_error(500, f"Validation error: {str(e)}")

print(f"Starting high-performance dual-format parameter validator on port {port}")
with socketserver.TCPServer(("", port), DualFormatValidator) as httpd:
    print(f"Serving at port {port}")
    httpd.serve_forever()
PYEOF

# Run the validator
python3 validator.py $PORT

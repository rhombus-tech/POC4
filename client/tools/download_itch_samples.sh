#!/bin/bash
# Script to download NASDAQ ITCH sample data files
# This script downloads sample ITCH data from NASDAQ's FTP server

# Create necessary directories
mkdir -p ../data/itch/samples
cd ../data/itch/samples

echo "Downloading NASDAQ ITCH sample data file..."
# Using anonymous FTP access
curl -o 20190327.PSX_ITCH_50.gz "ftp://anonymous:@emi.nasdaq.com/ITCH/Nasdaq%20PSX%20ITCH/20190327.PSX_ITCH_50.gz"

if [ $? -eq 0 ]; then
    echo "Download successful. Extracting file..."
    gunzip 20190327.PSX_ITCH_50.gz
    echo "Sample file extracted successfully at: $(pwd)/20190327.PSX_ITCH_50"
    echo "File size: $(du -h 20190327.PSX_ITCH_50 | cut -f1)"
else
    echo "Download failed. Please check the FTP server or try another sample file."
fi

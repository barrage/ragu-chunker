#!/bin/bash

if [[ " $@ " == *" -h "* ]]; then
	echo "Usage: source setup.sh [-h] [-c]"
	echo "  -h  Show this help message."
	echo "  -c  Clean up all environment variables set by this script (including .env)."
	return 0
fi

if [[ " $@ " == *" -c "* ]]; then
    echo "Cleaning up environment variables."
    unset DATABASE_URL
    unset QDRANT_URL
    unset WEAVIATE_URL
    unset OPENAI_KEY
    unset FEMBED_URL
    unset JWKS_ENDPOINT
    unset JWT_ISSUER
    unset ADDRESS
    unset UPLOAD_PATH
    unset CORS_ALLOWED_ORIGINS
    unset CORS_ALLOWED_HEADERS
    unset GOOGLE_DRIVE_DOWNLOAD_PATH
    echo "Cleaned up environment variables."
    return 0
fi

mkdir data &> /dev/null
if [[ $? == 0 ]]; then 
    echo "Created directory 'data'"
fi

mkdir data/upload &> /dev/null
if [[ $? == 0 ]]; then 
    echo "Created directory 'data/upload'"
fi

mkdir data/gdrive &> /dev/null
if [[ $? == 0 ]]; then 
    echo "Created directory 'data/gdrive'"
fi

if [[ -e .env ]]; then 
	source .env
else
	echo "No .env file found; Run 'cp .example.env .env' to create one."
	return 1
fi

docker compose up -d


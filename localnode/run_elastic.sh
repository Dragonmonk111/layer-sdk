#!/bin/bash

# Runs elastic search jaeger backend, but doesn't run the faucet

SCRIPT_DIR="$(realpath "$(dirname "$0")")"

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

cd "$SCRIPT_DIR"
$SUDO docker compose -f docker-compose.yml -f jaeger-elastic-compose.yml up -d

# Increase size of tags to be indexed, so that large values like "tx" can be searched
echo -n "Increasing tag size..."

SPAN_INDEX=$(until curl http://localhost:9200/_cat/indices 2> /dev/null | grep span | cut -d " " -f3 | grep -m 1 "jaeger-span-"; do sleep 1 ; done)

curl -sX PUT "localhost:9200/$SPAN_INDEX/_mapping?pretty" -H 'Content-Type: application/json' -d '{"dynamic_templates": [{"span_tags_map": {"path_match": "tag.*","mapping": {"ignore_above": 2048,"type": "keyword"}}}],"properties": {"tags": {"type": "nested","dynamic": "false","properties": {"key": {"type": "keyword","ignore_above": 2048},"tagType": {"type": "keyword","ignore_above": 2048},"value": {"type": "keyword","ignore_above": 2048}}}}}' > /dev/null

echo " done!"

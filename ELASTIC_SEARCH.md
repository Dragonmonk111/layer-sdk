# Elasticsearch queries

These have been tested with Postman, the API will probably use an Elasticsearch JS client. With the URLs shown below, 

## List indices

The root route defaults to the `service` index instead of the `span` index, which is what we usually need. So we list them to see how they are called since their name changes depending on the date of deployment.

Request URL: `http://localhost:9200/_cat/indices`

Response example:

```
green open jaeger-span-2024-09-12    NHSuiiRfR-2elT0mJecMpw 1 0 393743  0 19.1mb 19.1mb 19.1mb
green open jaeger-service-2024-09-12 Uvmk-GWtRW2Uh-owFHS2kA 1 0     15 52 14.4kb 14.4kb 14.4kb
```

## List traces

The `jaeger-span-<date>` index only serves spans. But we can get trace related data by querying root spans, those whose `references` field is an empty array, meaning they have no parent span.

Request URL: `http://localhost:9200/jaeger-span-2024-09-12/_search`

Request body:

```json
{
  "query": {
    "bool": {
      "must_not": [
        {
          "nested": {
            "path": "references",
            "query": {
              "exists": {
                "field": "references"
              }
            }
          }
        }
      ]
    }
  }
}
```

The returned response would be an array of spans, one span per trace on the DB.


# Get trace

Request URL: `http://localhost:9200/jaeger-span-2024-09-12/_search`

Request body:

```json
{
    "query": {
        "match": {
            "traceID": {
                "query": "3872e1704986f71e0fd51a5d4b7c3be6"
            }
        }
    }
}
```

The returned response would be an array with all of the spans that belong to the queried `traceID`.

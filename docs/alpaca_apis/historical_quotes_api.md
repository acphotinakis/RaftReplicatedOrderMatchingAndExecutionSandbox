---
updatedAt: 2026-05-27T17:58:03.000Z
agentTools:
  siteIndex: https://docs.alpaca.markets/llms.txt
  projectIndex: https://docs.alpaca.markets/us/llms.txt
---

# Historical trades

The historical stock trades API provides trade data for a list of stock symbols between the specified dates.

The returned results are sorted by symbol first then by trade timestamp. This means that you are likely to see only one symbol in your first response if there are enough trades for that symbol to hit the limit you requested.

In these situations, if you keep requesting again with the `next_page_token` from the previous response, you will eventually reach the other symbols if any trades were found for them.

# OpenAPI definition

```json
{
  "components": {
    "headers": {
      "ratelimit_limit": {
        "description": "Request limit per minute.",
        "example": 100,
        "schema": {
          "type": "integer"
        }
      },
      "ratelimit_remaining": {
        "description": "Request limit per minute remaining.",
        "example": 90,
        "schema": {
          "type": "integer"
        }
      },
      "ratelimit_reset": {
        "description": "The UNIX epoch when the remaining quota changes.",
        "example": 1674044551,
        "schema": {
          "type": "integer"
        }
      }
    },
    "parameters": {
      "end": {
        "description": "The inclusive end of the interval. Format: RFC-3339 or YYYY-MM-DD.\nDefault: the current time if the user has a real-time access for the feed, otherwise 15 minutes before the current time.\n",
        "examples": {
          "RFC-3339 nanosecond": {
            "summary": "RFC-3339 date-time with nanosecond accuracy",
            "value": "2024-01-04T01:02:03.123456789Z"
          },
          "RFC-3339 second": {
            "summary": "RFC-3339 date-time with second accuracy",
            "value": "2024-01-04T00:00:00Z"
          },
          "RFC-3339 with timezone": {
            "summary": "RFC-3339 date-time with time zone",
            "value": "2024-01-04T09:30:00-04:00"
          },
          "date": {
            "summary": "Date",
            "value": "2024-01-04"
          }
        },
        "in": "query",
        "name": "end",
        "required": false,
        "schema": {
          "format": "date-time",
          "type": "string"
        }
      },
      "limit": {
        "description": "The maximum number of data points to return in the response page.\nThe API may return less, even if there are more available data points in the requested interval.\nAlways check the `next_page_token` for more pages.\nThe limit applies to the total number of data points, not per symbol!\n",
        "in": "query",
        "name": "limit",
        "required": false,
        "schema": {
          "default": 1000,
          "maximum": 10000,
          "minimum": 1,
          "type": "integer"
        }
      },
      "page_token": {
        "description": "The pagination token from which to continue. The value to pass here is returned in specific requests when more data is available, usually because of a response result limit.\n",
        "in": "query",
        "name": "page_token",
        "schema": {
          "type": "string"
        }
      },
      "sort": {
        "description": "Sort data in ascending or descending order.",
        "in": "query",
        "name": "sort",
        "schema": {
          "allOf": [
            {
              "type": "string"
            },
            {
              "$ref": "#/components/schemas/sort"
            }
          ]
        }
      },
      "start": {
        "description": "The inclusive start of the interval. Format: RFC-3339 or YYYY-MM-DD.\nDefault: the beginning of the current day, but at least 15 minutes ago if the user doesn't have real-time access for the feed.\n",
        "examples": {
          "RFC-3339 nanosecond": {
            "summary": "RFC-3339 date-time with nanosecond accuracy",
            "value": "2024-01-03T01:02:03.123456789Z"
          },
          "RFC-3339 second": {
            "summary": "RFC-3339 date-time with second accuracy",
            "value": "2024-01-03T00:00:00Z"
          },
          "RFC-3339 with timezone": {
            "summary": "RFC-3339 date-time with time zone",
            "value": "2024-01-03T09:30:00-04:00"
          },
          "date": {
            "summary": "Date",
            "value": "2024-01-03"
          }
        },
        "in": "query",
        "name": "start",
        "required": false,
        "schema": {
          "format": "date-time",
          "type": "string"
        }
      },
      "stock_asof": {
        "description": "The as-of date of the queried stock symbol(s). Format: YYYY-MM-DD. Default: current day.\n\nThis date is used to identify the underlying entity of the provided symbol(s), so that name changes for this entity can be found. Data for past symbol(s) is returned if the query date range spans the name change.\n\nThe special value of \"-\" means symbol mapping is skipped. Data is returned based on the symbol alone without looking up previous names. The same happens if the queried symbol is not found on the given `asof` date.\n\nExample: FB was renamed to META in 2022-06-09. Querying META with an `asof` date after 2022-06-09 will also yield FB data. The data for the FB ticker will be labeled as META because they are considered the same underlying entity as of 2022-06-09. Querying FB with an `asof` date after 2022-06-09 will only return data with the FB ticker, not with META. But with an `asof` date before 2022-06-09, META will also be returned (as FB).\n",
        "in": "query",
        "name": "asof",
        "schema": {
          "type": "string"
        }
      },
      "stock_currency": {
        "description": "The currency of all prices in ISO 4217 format. Default: USD.\n",
        "in": "query",
        "name": "currency",
        "schema": {
          "type": "string"
        }
      },
      "stock_historical_feed": {
        "description": "The source feed of the data.\n - `sip`: all US exchanges\n - `iex`: Investors EXchange\n - `boats`: Blue Ocean ATS, overnight US trading data\n - `otc`: over-the-counter exchanges\n",
        "in": "query",
        "name": "feed",
        "schema": {
          "$ref": "#/components/schemas/stock_historical_feed"
        }
      },
      "stock_symbols": {
        "description": "A comma-separated list of stock symbols.",
        "example": "AAPL,TSLA",
        "in": "query",
        "name": "symbols",
        "required": true,
        "schema": {
          "type": "string"
        }
      }
    },
    "responses": {
      "400": {
        "description": "One of the request parameters is invalid. See the returned message for details.\n",
        "headers": {
          "X-RateLimit-Limit": {
            "$ref": "#/components/headers/ratelimit_limit"
          },
          "X-RateLimit-Remaining": {
            "$ref": "#/components/headers/ratelimit_remaining"
          },
          "X-RateLimit-Reset": {
            "$ref": "#/components/headers/ratelimit_reset"
          }
        }
      },
      "401": {
        "description": "Authentication headers are missing or invalid. Make sure you authenticate your request with a valid API key.\n"
      },
      "403": {
        "description": "The requested resource is forbidden.\n"
      },
      "429": {
        "description": "Too many requests. You hit the rate limit. Use the X-RateLimit-... response headers to make sure you're under the rate limit.\n",
        "headers": {
          "X-RateLimit-Limit": {
            "$ref": "#/components/headers/ratelimit_limit"
          },
          "X-RateLimit-Remaining": {
            "$ref": "#/components/headers/ratelimit_remaining"
          },
          "X-RateLimit-Reset": {
            "$ref": "#/components/headers/ratelimit_reset"
          }
        }
      },
      "500": {
        "description": "Internal server error. We recommend retrying these later. If the issue persists, please contact us on [Slack](https://alpaca.markets/slack) or on the [Community Forum](https://forum.alpaca.markets/).\n"
      }
    },
    "schemas": {
      "next_page_token": {
        "description": "Pagination token for the next page.",
        "type": [
          "string",
          "null"
        ]
      },
      "sort": {
        "default": "asc",
        "description": "Sort data in ascending or descending order.",
        "enum": [
          "asc",
          "desc"
        ],
        "type": "string"
      },
      "stock_historical_feed": {
        "default": "sip",
        "enum": [
          "iex",
          "otc",
          "sip",
          "boats"
        ],
        "type": "string"
      },
      "stock_tape": {
        "description": "- A: New York Stock Exchange\n- B: NYSE Arca, Bats, IEX and other regional exchanges\n- C: NASDAQ\n- N: Overnight\n- O: OTC\n",
        "enum": [
          "A",
          "B",
          "C",
          "N",
          "O"
        ],
        "type": "string"
      },
      "stock_trade": {
        "description": "A stock trade.",
        "examples": [
          {
            "c": [
              "@",
              "T"
            ],
            "i": 1,
            "p": 178.26,
            "s": 246,
            "t": "2022-01-03T09:00:00.086175744Z",
            "x": "P",
            "z": "C"
          }
        ],
        "properties": {
          "c": {
            "description": "Condition flags. See `v2/stocks/meta/conditions/trade` for more details.",
            "items": {
              "type": "string"
            },
            "type": "array"
          },
          "i": {
            "description": "Trade ID sent by the exchange.",
            "format": "uint64",
            "type": "integer"
          },
          "p": {
            "description": "Trade price.",
            "format": "double",
            "type": "number"
          },
          "s": {
            "description": "Trade size.",
            "format": "uint32",
            "type": "integer"
          },
          "t": {
            "$ref": "#/components/schemas/timestamp"
          },
          "u": {
            "description": "Update to the trade. This field is optional, if it's missing, the trade is valid. Otherwise, it can have these values:\n - canceled: indicates that the trade has been canceled\n - incorrect: indicates that the trade has been corrected and the given trade is no longer valid\n - corrected: indicates that this trade is the correction of a previous (incorrect) trade\n",
            "type": "string"
          },
          "x": {
            "description": "Exchange code. See `v2/stocks/meta/exchanges` for more details.",
            "type": "string"
          },
          "z": {
            "$ref": "#/components/schemas/stock_tape"
          }
        },
        "required": [
          "t",
          "i",
          "x",
          "p",
          "s",
          "c",
          "z"
        ],
        "type": "object"
      },
      "stock_trades_resp": {
        "properties": {
          "currency": {
            "type": "string"
          },
          "next_page_token": {
            "$ref": "#/components/schemas/next_page_token"
          },
          "trades": {
            "additionalProperties": {
              "items": {
                "$ref": "#/components/schemas/stock_trade"
              },
              "type": "array"
            },
            "type": "object"
          }
        },
        "required": [
          "trades",
          "next_page_token"
        ],
        "type": "object"
      },
      "timestamp": {
        "description": "Timestamp in RFC-3339 format with nanosecond precision.",
        "format": "date-time",
        "type": "string"
      }
    },
    "securitySchemes": {
      "BasicAuth": {
        "scheme": "basic",
        "type": "http"
      },
      "apiKey": {
        "in": "header",
        "name": "APCA-API-KEY-ID",
        "type": "apiKey"
      },
      "apiSecret": {
        "in": "header",
        "name": "APCA-API-SECRET-KEY",
        "type": "apiKey"
      }
    }
  },
  "info": {
    "contact": {
      "email": "support@alpaca.markets",
      "name": "Alpaca Support",
      "url": "https://alpaca.markets/support"
    },
    "description": "Access real-time and historical market data for US equities, options, crypto, and foreign exchange data through the Alpaca REST and WebSocket APIs. There are APIs for Stock Pricing, Option Pricing, Crypto Pricing, Forex, Logos, Fixed income, Corporate Actions, Screener, and News.\n",
    "license": {
      "name": "Creative Commons Attribution Share Alike 4.0 International",
      "url": "https://spdx.org/licenses/CC-BY-SA-4.0.html"
    },
    "termsOfService": "https://s3.amazonaws.com/files.alpaca.markets/disclosures/library/TermsAndConditions.pdf",
    "title": "Market Data API",
    "version": "1.1"
  },
  "openapi": "3.1.2",
  "paths": {
    "/v2/stocks/trades": {
      "get": {
        "description": "The historical stock trades API provides trade data for a list of stock symbols between the specified dates.\n\nThe returned results are sorted by symbol first then by trade timestamp. This means that you are likely to see only one symbol in your first response if there are enough trades for that symbol to hit the limit you requested.\n\nIn these situations, if you keep requesting again with the `next_page_token` from the previous response, you will eventually reach the other symbols if any trades were found for them.",
        "operationId": "StockTrades",
        "parameters": [
          {
            "$ref": "#/components/parameters/stock_symbols"
          },
          {
            "$ref": "#/components/parameters/start"
          },
          {
            "$ref": "#/components/parameters/end"
          },
          {
            "$ref": "#/components/parameters/limit"
          },
          {
            "$ref": "#/components/parameters/stock_asof"
          },
          {
            "$ref": "#/components/parameters/stock_historical_feed"
          },
          {
            "$ref": "#/components/parameters/stock_currency"
          },
          {
            "$ref": "#/components/parameters/page_token"
          },
          {
            "$ref": "#/components/parameters/sort"
          }
        ],
        "responses": {
          "200": {
            "content": {
              "application/json": {
                "examples": {
                  "trades": {
                    "value": {
                      "next_page_token": "QUFQTHwyMDIyLTAxLTAzVDA5OjAwOjAwLjI0NDgzOTY4MFp8UHwwOTIyMzM3MjAzNjg1NDc3NTgxMA==",
                      "trades": {
                        "AAPL": [
                          {
                            "c": [
                              "@",
                              "T"
                            ],
                            "i": 1,
                            "p": 178.26,
                            "s": 246,
                            "t": "2022-01-03T09:00:00.086175744Z",
                            "x": "P",
                            "z": "C"
                          },
                          {
                            "c": [
                              "@",
                              "F",
                              "T",
                              "I"
                            ],
                            "i": 2,
                            "p": 178.26,
                            "s": 1,
                            "t": "2022-01-03T09:00:00.24483968Z",
                            "x": "P",
                            "z": "C"
                          }
                        ]
                      }
                    }
                  }
                },
                "schema": {
                  "$ref": "#/components/schemas/stock_trades_resp"
                }
              }
            },
            "description": "OK",
            "headers": {
              "X-RateLimit-Limit": {
                "$ref": "#/components/headers/ratelimit_limit"
              },
              "X-RateLimit-Remaining": {
                "$ref": "#/components/headers/ratelimit_remaining"
              },
              "X-RateLimit-Reset": {
                "$ref": "#/components/headers/ratelimit_reset"
              }
            }
          },
          "400": {
            "$ref": "#/components/responses/400"
          },
          "401": {
            "$ref": "#/components/responses/401"
          },
          "403": {
            "$ref": "#/components/responses/403"
          },
          "429": {
            "$ref": "#/components/responses/429"
          },
          "500": {
            "$ref": "#/components/responses/500"
          }
        },
        "security": [
          {
            "apiKey": [],
            "apiSecret": []
          },
          {
            "BasicAuth": []
          }
        ],
        "summary": "Historical trades",
        "tags": [
          "Stock"
        ]
      }
    }
  },
  "servers": [
    {
      "description": "Production",
      "url": "https://data.alpaca.markets"
    },
    {
      "description": "Sandbox",
      "url": "https://data.sandbox.alpaca.markets"
    }
  ],
  "tags": [
    {
      "description": "Endpoints for stocks.",
      "name": "Stock"
    }
  ]
}
```

Historical quotes
GET
https://data.alpaca.markets/v2/stocks/quotes


The historical stock quotes API provides quote data for a list of stock symbols between the specified dates.

The returned results are sorted by symbol first, then by the quote timestamp. This means that you are likely to see only one symbol in your first response if there are enough quotes for that symbol to hit the limit you requested.

In these situations, if you keep requesting again with the next_page_token from the previous response, you will eventually reach the other symbols if any quotes were found for them.

Query Params
symbols
string
required

A comma-separated list of stock symbols.
start
date-time

The inclusive start of the interval. Format: RFC-3339 or YYYY-MM-DD.
Default: the beginning of the current day, but at least 15 minutes ago if the user doesn't have real-time access for the feed.
end
date-time

The inclusive end of the interval. Format: RFC-3339 or YYYY-MM-DD.
Default: the current time if the user has a real-time access for the feed, otherwise 15 minutes before the current time.
limit
integer
1 to 10000
Defaults to 1000

The maximum number of data points to return in the response page.
The API may return less, even if there are more available data points in the requested interval.
Always check the next_page_token for more pages.
The limit applies to the total number of data points, not per symbol!
asof
string

The as-of date of the queried stock symbol(s). Format: YYYY-MM-DD. Default: current day.

This date is used to identify the underlying entity of the provided symbol(s), so that name changes for this entity can be found. Data for past symbol(s) is returned if the query date range spans the name change.

The special value of "-" means symbol mapping is skipped. Data is returned based on the symbol alone without looking up previous names. The same happens if the queried symbol is not found on the given asof date.

Example: FB was renamed to META in 2022-06-09. Querying META with an asof date after 2022-06-09 will also yield FB data. The data for the FB ticker will be labeled as META because they are considered the same underlying entity as of 2022-06-09. Querying FB with an asof date after 2022-06-09 will only return data with the FB ticker, not with META. But with an asof date before 2022-06-09, META will also be returned (as FB).
feed
string
enum
Defaults to sip

The source feed of the data.

sip: all US exchanges
iex: Investors EXchange
boats: Blue Ocean ATS, overnight US trading data
otc: over-the-counter exchanges

Allowed:

iex

otc

sip

boats
currency
string

The currency of all prices in ISO 4217 format. Default: USD.
page_token
string

The pagination token from which to continue. The value to pass here is returned in specific requests when more data is available, usually because of a response result limit.
sort
string
enum
Defaults to asc

Sort data in ascending or descending order.

Allowed:

asc

desc

Responses


200
OK



400
One of the request parameters is invalid. See the returned message for details.


401
Authentication headers are missing or invalid. Make sure you authenticate your request with a valid API key.

403
The requested resource is forbidden.


429
Too many requests. You hit the rate limit. Use the X-RateLimit-... response headers to make sure you're under the rate limit.


500
Internal server error. We recommend retrying these later. If the issue persists, please contact us on Slack or on the Community Forum.
Exchange codes
Latest quotes
Did this page help you?

# generic-web-sraper
Recursive and target agnostic web scraper with declarative configuration. The user should specify JSON configuration which describes desired workflow specification.
The most of the configuration file consists of a tree-like structure, where each node can represent the following: 
- Scraping target configuration: URLs with possible authentication, query parameters, headers, etc.
- Processing details: type of processing (either HTML, Regex, JSON) with corresponding config. 
- Storage location: either local or GoogleDrive.

Scraping process is executed in recursive manner where every subsequent scrape/processing/storage job is derived from the results of previous one. Moreover every new bit of work should be sent to specified stream in order to be executed. This preserves isolation of each scraper job during the workflow execution.

## Example of JSON configuration:
```json
{
    "urls":[
      "https://dummy.website.com/path"
    ],
    "scraper":{
      "dynamic_parameters":{
        "IntRange":{
          "name":{
            "Name":"page"
          },
          "start":1,
          "end":10,
          "step":1
        }
      },
      "targets":{
        "Text":[
          {
            "Proc":{
              "type":"Html",
              "selector":"div.some-class td.title-cell a",
              "capture_elements":"All",
              "selector_target":{
                "Attr":"href"
              },
              "proc_result":"URL",
              "next_steps":[
                {
                  "Scrape":{
                    "targets":{
                      "Text":[
                        {
                          "Process":{
                            "type":"Html",
                            "selector":".some-container img",
                            "capture_elements":"All",
                            "selector_target":{
                              "Attr":"src"
                            },
                            "proc_result":"URL",
                            "next_steps":[
                              {
                                "Scrape":{
                                  "targets":{
                                    "Bytes":[
                                      {
                                        "Store":{
                                          "LocalDrive":{
                                            "dirname":{
                                              "path":"path/to/somewhere/",
                                              "or_create":true
                                            },
                                            "filename_class":"Origin"
                                          }
                                        }
                                      }
                                    ]
                                  }
                                }
                              }
                            ]
                          }
                        }
                      ]
                    }
                  }
                }
              ]
            }
          }
        ]
      }
    }
  }

```

---
title: Tool Calling
description: NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Tool Calling
order: 2
---

To give your LLM the ability to interact with the outside world, you will need tool calling.

## Declaring a tool
A tool can be created from any (synchronous) python function, which returns a string.
To perform the conversion, all that is needed, is using simple `@tool` decorator. To get
a good sense of how such a tool can look like, consider this arbitrary weather scenario:
```python
import requests
from nobodywho import tool

FORECAST_URL = "https://api.open-meteo.com/v1/forecast"

@tool(description="Given coordinates, gets the current temperature.")
def get_current_temperature(lon: str, lat: str) -> str:
    res = requests.get(
        url=FORECAST_URL,
        params={"latitude": lat, "longitude": lon, "current_weather": True}
    ).json()

    return str(res.current_weather.temperature)
```
As you can see, every `@tool` definition has to be complemented by a description
of what such tool does. To let your LLM use it, simply add it when creating `Chat`:
```python
chat = Chat('./model.gguf', tools=[get_current_temperature])
```
NobodyWho then figures out the right tool calling format, infers the parameters of types
and configures the sampler so that when the model decides to use tools, it will adhere to the format.

Naturally, more tools can be defined and the model can chain the calls for them:
```python
import requests
from nobodywho import Chat, tool

FORECAST_URL = "https://api.open-meteo.com/v1/forecast"
GEOLOCATION_URL = "https://geocoding-api.open-meteo.com/v1/search"

@tool(description="Given a longitude and latitude, gets the current temperature.")
def get_current_temperature(lon: str, lat: str) -> str:
    res = requests.get(
        url=FORECAST_URL,
        params={"latitude": lat, "longitude": lon, "current_weather": True}
    ).json()

    return str(res.current_weather.temperature)

@tool(description="Given a city name, gives you the longitude and latitude.")
def get_city_position(city: str) -> str:
    position_res = requests.get(
        url=GEOLOCATION_URL,
        params={"name": city, "count": 1}
    ).json()

    lat = position_res.results[0].latitude
    lon = position_res.results[0].longitude

    return f"Longitude: {lon}, latitude: {lat}"

chat = Chat('./model.gguf', tools=[get_city_position, get_current_temperature])
response = chat.ask('What is the current temperature in Copenhagen?').complete()
print(response) # It is ... degrees in Copenhagen!
```

!!! info ""
    Note that **not every model** supports tool calling. If the model does not have
    such an option, it might not call your tools.

## Providing params descriptions

When a tool call is declared, information about the description, the types and the parameters is provided to the model, so it knows it can use it. Crucially, also parameter names are provided.

If those are not enough, you can decide to provide additional information by the `params` parameter:
```python
@tool(
    description="Given a longitude and latitude, gets the current temperature."
    params={
        "lon": "Longitude - that is the vertical one!"
        "lat": "Latitude - that is the horizontal one!"
    }
)
def get_current_temperature(lon: str, lat: str) -> str:
    ...
```
These will be then appended to the information provided to model, so it can better navigate itself
when using the tool.

## Converting from JSON schema

<div style="background-color: red;">
    TODO: Enable smoother reset, with defaults. This is clunky.
</div>

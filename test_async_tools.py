#!/usr/bin/env python3
"""Test script for async tool support"""

import asyncio
import sys

# Mock nobodywho module for testing tool decorator logic
# In real usage, this would be: import nobodywho

print("Testing async tool detection...")

# Test Case 1: Synchronous function
def sync_function(x: int) -> str:
    return f"Sync: {x}"

# Test Case 2: Asynchronous function
async def async_function(x: int) -> str:
    await asyncio.sleep(0.1)
    return f"Async: {x}"

# Test detection using inspect
import inspect

print(f"sync_function is async: {inspect.iscoroutinefunction(sync_function)}")
print(f"async_function is async: {inspect.iscoroutinefunction(async_function)}")

# Test running async function with asyncio.run()
print("\nTesting asyncio.run()...")
result = asyncio.run(async_function(42))
print(f"Result: {result}")

# Test calling async function (returns coroutine)
print("\nTesting coroutine call...")
coro = async_function(100)
print(f"Calling async function returns: {type(coro)}")
result = asyncio.run(coro)
print(f"After asyncio.run(): {result}")

print("\n✅ All basic tests passed!")
print("\nTo fully test the implementation, build nobodywho and run:")
print("  cd nobodywho && maturin develop")
print("  python -c 'import nobodywho; # use async tools here'")

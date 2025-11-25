#!/usr/bin/env python3
"""
Simple MCP (Model Context Protocol) stdio server example.

This is a minimal MCP server that communicates over stdin/stdout using JSON-RPC 2.0.
It implements a basic calculator with add, subtract, multiply, and divide operations.
"""

import sys
import json


def handle_tools_list(request_id):
    """Return the list of available tools."""
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "tools": [
                {
                    "name": "add",
                    "description": "Add two numbers",
                    "inputs": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number", "description": "First number"},
                            "b": {"type": "number", "description": "Second number"}
                        },
                        "required": ["a", "b"]
                    },
                    "outputs": {"type": "number"},
                    "tags": ["math", "calculator"]
                },
                {
                    "name": "subtract",
                    "description": "Subtract two numbers",
                    "inputs": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number", "description": "First number"},
                            "b": {"type": "number", "description": "Second number"}
                        },
                        "required": ["a", "b"]
                    },
                    "outputs": {"type": "number"},
                    "tags": ["math", "calculator"]
                },
                {
                    "name": "multiply",
                    "description": "Multiply two numbers",
                    "inputs": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number", "description": "First number"},
                            "b": {"type": "number", "description": "Second number"}
                        },
                        "required": ["a", "b"]
                    },
                    "outputs": {"type": "number"},
                    "tags": ["math", "calculator"]
                },
                {
                    "name": "divide",
                    "description": "Divide two numbers",
                    "inputs": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number", "description": "Numerator"},
                            "b": {"type": "number", "description": "Denominator"}
                        },
                        "required": ["a", "b"]
                    },
                    "outputs": {"type": "number"},
                    "tags": ["math", "calculator"]
                }
            ]
        }
    }


def handle_tools_call(request_id, params):
    """Execute a tool call."""
    tool_name = params.get("name")
    arguments = params.get("arguments", {})
    
    try:
        a = arguments.get("a")
        b = arguments.get("b")
        
        if a is None or b is None:
            return {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32602,
                    "message": "Missing required arguments 'a' and 'b'"
                }
            }
        
        # Perform the calculation
        if tool_name == "add":
            result = a + b
        elif tool_name == "subtract":
            result = a - b
        elif tool_name == "multiply":
            result = a * b
        elif tool_name == "divide":
            if b == 0:
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {
                        "code": -32000,
                        "message": "Division by zero"
                    }
                }
            result = a / b
        else:
            return {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32601,
                    "message": f"Unknown tool: {tool_name}"
                }
            }
        
        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "result": result,
                "tool": tool_name,
                "arguments": arguments
            }
        }
    except Exception as e:
        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {
                "code": -32000,
                "message": str(e)
            }
        }


def main():
    """Main event loop for the MCP server."""
    # Log to stderr so it doesn't interfere with JSON-RPC on stdout
    print("MCP Calculator Server started", file=sys.stderr)
    
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        
        try:
            request = json.loads(line)
            method = request.get("method")
            request_id = request.get("id", 1)
            params = request.get("params", {})
            
            # Handle different MCP methods
            if method == "tools/list":
                response = handle_tools_list(request_id)
            elif method == "tools/call":
                response = handle_tools_call(request_id, params)
            else:
                response = {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {
                        "code": -32601,
                        "message": f"Method not found: {method}"
                    }
                }
            
            # Send response to stdout
            print(json.dumps(response), flush=True)
            
        except json.JSONDecodeError as e:
            error_response = {
                "jsonrpc": "2.0",
                "id": None,
                "error": {
                    "code": -32700,
                    "message": f"Parse error: {e}"
                }
            }
            print(json.dumps(error_response), flush=True)
        except Exception as e:
            error_response = {
                "jsonrpc": "2.0",
                "id": None,
                "error": {
                    "code": -32603,
                    "message": f"Internal error: {e}"
                }
            }
            print(json.dumps(error_response), flush=True)


if __name__ == "__main__":
    main()

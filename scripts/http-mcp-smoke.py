#!/usr/bin/env python3
"""Read-only Streamable HTTP MCP initialization/discovery check."""
import json,os,urllib.request

url=os.environ.get('PINBOARD_TEST_URL','http://127.0.0.1:4127')+'/mcp'
session=None
def rpc(method,number=None,params=None):
    global session
    body=dict(jsonrpc='2.0',method=method)
    if number is not None:body['id']=number
    if params is not None:body['params']=params
    headers={'Content-Type':'application/json','Accept':'application/json, text/event-stream','MCP-Protocol-Version':'2025-06-18'}
    if session:headers['Mcp-Session-Id']=session
    with urllib.request.urlopen(urllib.request.Request(url,json.dumps(body).encode(),headers),timeout=15) as response:
        session=response.headers.get('Mcp-Session-Id',session)
        text=response.read().decode()
        if not text:return None
        if response.headers.get('Content-Type','').startswith('text/event-stream'):
            text=next(line[6:] for line in text.splitlines() if line.startswith('data: '))
        result=json.loads(text);assert 'error' not in result,result;return result['result']

info=rpc('initialize',1,dict(protocolVersion='2025-06-18',capabilities={},clientInfo=dict(name='pinboard-http-smoke',version='1.0')))
assert info['serverInfo']['name']=='pinboard-mcp'
rpc('notifications/initialized')
assert len(rpc('tools/list',2)['tools'])==11
assert 'resources' in rpc('resources/list',3)
print('PASS: Streamable HTTP initialization, session handling, tool discovery, and resources')

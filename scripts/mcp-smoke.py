#!/usr/bin/env python3
"""Exercise a real stdio MCP session against a disposable Turso database."""
import json, os, pathlib, select, subprocess, tempfile

root=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='pinboard-mcp-') as temp:
    process=subprocess.Popen([str(root/'target/debug/pinboard-mcp'),'--db',temp+'/test.db','--port','0','--no-demo'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1,cwd=root,env={k:v for k,v in os.environ.items() if not k.startswith(('TURSO_', 'PINBOARD_OIDC_', 'PINBOARD_SAML_', 'PINBOARD_SMTP_')) and k not in ('PINBOARD_PUBLIC_URL','PINBOARD_STDIO_TOKEN')})
    def send(message):
        process.stdin.write(json.dumps(message)+'\n');process.stdin.flush()
    def call(number,method,params=None):
        send(dict(jsonrpc='2.0',id=number,method=method,**({'params':params} if params is not None else {})))
        while True:
            if not select.select([process.stdout],[],[],15)[0]:
                raise AssertionError(f'MCP timeout: {method}')
            line=process.stdout.readline()
            assert line, f'MCP exited: {process.poll()}'
            message=json.loads(line)
            if message.get('id')==number:return message
    def tool(number,name,args):
        result=call(number,'tools/call',dict(name=name,arguments=args))
        assert 'result' in result, result
        assert not result['result'].get('isError'),result
        return json.loads(result['result']['content'][0]['text'])
    try:
        initialized=call(1,'initialize',dict(protocolVersion='2025-06-18',capabilities={},clientInfo=dict(name='pinboard-smoke',version='1.0')))
        assert 'result' in initialized,initialized
        send(dict(jsonrpc='2.0',method='notifications/initialized'))
        listed=call(2,'tools/list')['result']['tools']
        assert {t['name'] for t in listed}=={'list_canvases','create_canvas','read_canvas','edit_canvas','export_canvas','propose_gap','undo_canvas','read_comments','comment_canvas','reply_comment','resolve_comment'}
        node=lambda id,title,x:dict(id=id,title=title,kind='step',x=x,y=50,width=220,height=140)
        created=tool(3,'create_canvas',dict(title='MCP plan',nodes=[node('research','Research',0),node('ship','Ship',300)],edges=[dict(id='flow',**{'from':'research','to':'ship'})]))
        board=created['canvas'];assert created['url'].startswith('http://127.0.0.1:')
        selected=tool(4,'read_canvas',dict(canvas_id=board['id'],region=dict(x=0,y=0,width=230,height=200)))
        assert selected['selected_ids']==['research']
        changed=tool(5,'edit_canvas',dict(canvas_id=board['id'],base_revision=0,scope=['research'],operations=[dict(op='update_node',id='research',changes=dict(owner='Avery',due='2026-12-01'))]))
        assert changed['revision']==1 and changed['nodes'][0]['owner']=='Avery'
        rejected=call(6,'tools/call',dict(name='edit_canvas',arguments=dict(canvas_id=board['id'],base_revision=1,scope=['research'],operations=[dict(op='delete_node',id='research')])))
        assert 'error' in rejected or rejected.get('result',{}).get('isError'),rejected
        resource=call(7,'resources/read',dict(uri='canvas://'+board['id']))
        persisted=json.loads(resource['result']['contents'][0]['text']);assert persisted['revision']==1 and len(persisted['edges'])==1
        exported=tool(8,'export_canvas',dict(canvas_id=board['id'],format='markdown'));assert '# MCP plan' in exported['content'] and 'Avery' in exported['content']
        stale=call(9,'tools/call',dict(name='edit_canvas',arguments=dict(canvas_id=board['id'],base_revision=0,operations=[dict(op='rename',title='Stale')])));assert 'error' in stale or stale.get('result',{}).get('isError'),stale
        assert len(call(10,'resources/list')['result']['resources'])==1
        proposed=tool(11,'propose_gap',dict(canvas_id=board['id'],base_revision=1,**{'from':'research','to':'ship'},title='Approval',rationale='Review before shipping'))
        suggestion=next(n for n in proposed['nodes'] if n['suggested']);assert len(proposed['edges'])==1
        accepted=tool(12,'edit_canvas',dict(canvas_id=board['id'],base_revision=2,operations=[dict(op='accept_suggestion',id=suggestion['id'])]));assert len(accepted['edges'])==2
        undone=tool(13,'undo_canvas',dict(canvas_id=board['id']));assert next(n for n in undone['nodes'] if n['id']==suggestion['id'])['suggested'] and len(undone['edges'])==1
        undone=tool(14,'undo_canvas',dict(canvas_id=board['id']));assert len(undone['nodes'])==2 and undone['nodes'][0]['owner']=='Avery'
        thread=tool(15,'comment_canvas',dict(canvas_id=board['id'],anchor_id='research',x=0,y=0,body='A review note'))
        replied=tool(16,'reply_comment',dict(canvas_id=board['id'],thread_id=thread['id'],body='Follow-up'));assert len(replied['replies'])==2
        tool(17,'resolve_comment',dict(canvas_id=board['id'],thread_id=thread['id'],resolved=True))
        assert tool(18,'read_comments',dict(canvas_id=board['id']))[0]['resolved']
        print('PASS: stdio initialization, discovery, create, region scope, atomic edit, resources, exports, proposals, author-local undo, attributed comments, and stale revision rejection')
    finally:
        process.stdin.close()
        try:process.wait(timeout=5)
        except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=5)

from __future__ import annotations

import asyncio
from pathlib import Path
from aiohttp import web
from .appliance import EventStore
from .storage import VestaForgeStore

EVENTS = web.AppKey("events", EventStore)
RADIO = web.AppKey("radio", object)


@web.middleware
async def guard(request, handler):
    try:
        if request.path not in ('/health', '/public/announcements', '/public', '/staff', '/'):
            header = request.headers.get('Authorization', '')
            actor = request.app[EVENTS].authenticate(header[7:]) if header.startswith('Bearer ') else None
            if not actor:
                raise web.HTTPUnauthorized()
            request['actor'] = actor
        response = await handler(request)
    except (ValueError, TypeError, KeyError) as exc:
        response = web.json_response({'error': str(exc)}, status=400)
    except web.HTTPException as exc:
        response = web.json_response({'error': exc.reason}, status=exc.status)
    response.headers['Cache-Control'] = 'no-store'
    response.headers['X-Content-Type-Options'] = 'nosniff'
    response.headers['Content-Security-Policy'] = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'"
    return response


async def body(request):
    try:
        value = await request.json()
    except (ValueError, UnicodeError):
        raise ValueError('invalid JSON')
    if not isinstance(value, dict):
        raise ValueError('JSON object required')
    return value


def operator(request):
    if request['actor']['role'] != 'operator':
        raise web.HTTPForbidden()


def create_app(store: VestaForgeStore, config):
    app = web.Application(middlewares=[guard], client_max_size=16384)
    events = EventStore(store.state_dir, config.get('queueLimit', 1000))
    app[EVENTS] = events

    async def health(request):
        return web.json_response({'ok': True, 'coordinator': config.get('coordinatorId', 'mish')})

    async def page(request):
        return web.FileResponse(Path(__file__).parent / 'static' / ('public.html' if request.path == '/public' else 'staff.html'))

    async def public(request):
        return web.json_response({'announcements': events.public()})

    async def operational_status(request):
        worker = request.app.get(RADIO)
        return web.json_response({'radio': worker.status if worker else 'starting',
            'adapter': worker.adapter if worker else 'disabled'})

    async def messages(request):
        if request.method == 'GET':
            return web.json_response({'messages': events.records()})
        value = await body(request)
        return web.json_response(events.create(value.get('text'), request['actor']['id']), status=201)

    async def alert(request):
        value = await body(request)
        return web.json_response(events.create(value.get('text'), request['actor']['id'],
            value.get('kind', 'alert'), value.get('responder'), value.get('expires')), status=201)

    async def ack(request):
        return web.json_response(events.ack(request.match_info['id'], request['actor']['id']))

    async def enroll(request):
        operator(request)
        value = await body(request)
        return web.json_response(events.enroll(value.get('name'), value.get('role', 'staff')), status=201)

    async def revoke(request):
        operator(request)
        events.revoke(request.match_info['id'])
        return web.json_response({'revoked': request.match_info['id']})

    async def publish(request):
        operator(request)
        value = await body(request)
        return web.json_response(events.publish(value.get('text')), status=201)

    async def legacy(request):
        operator(request)
        if request.method == 'GET':
            return web.json_response({'waypoints': store.list_waypoints()} if request.path == '/waypoints' else {'geopacks': store.list_geopacks()})
        value = await body(request)
        if request.path == '/waypoints/simulated':
            if not config.get('radio', {}).get('simulator', {}).get('enable', False):
                raise web.HTTPForbidden(reason='simulation disabled')
            position = value.get('position', {})
            if not isinstance(position, dict):
                raise ValueError('position must be an object')
            item, created = store.add_simulated_waypoint(lat=value.get('lat', position.get('lat')), lon=value.get('lon', position.get('lon')), label=value.get('label'), body=value.get('body'), visibility=value.get('visibility', 'private'), waypoint_id=value.get('id'), source_payload=value.get('payload', {}))
        elif request.path == '/geopacks':
            item, created = store.create_geopack(value.get('name', ''), value.get('waypointIds', []), value.get('visibility', 'private'), value.get('id'))
        else:
            # Network publication stays a local CLI operation during the PoC.
            raise web.HTTPNotFound()
        return web.json_response({'item': item, 'created': created}, status=201 if created else 200)

    app.router.add_get('/health', health)
    for route in ('/', '/staff', '/public'):
        app.router.add_get(route, page)
    app.router.add_get('/public/announcements', public)
    app.router.add_get('/api/status', operational_status)
    app.router.add_get('/api/messages', messages)
    app.router.add_post('/api/messages', messages)
    app.router.add_post('/api/alerts', alert)
    app.router.add_post('/api/alerts/{id}/ack', ack)
    app.router.add_post('/api/staff', enroll)
    app.router.add_post('/api/announcements', publish)
    app.router.add_delete('/api/staff/{id}', revoke)
    for route in ('/waypoints', '/geopacks'):
        app.router.add_get(route, legacy)
    app.router.add_post('/waypoints/simulated', legacy)
    app.router.add_post('/geopacks', legacy)

    async def lifecycle(app):
        from .radio import RadioWorker
        worker = RadioWorker(events, config)
        app[RADIO] = worker
        worker.start()
        async def retention():
            while True:
                events.prune(config.get('retentionDays', 7))
                await asyncio.sleep(3600)
        task = asyncio.create_task(retention())
        try:
            yield
        finally:
            task.cancel()
            await asyncio.gather(task, return_exceptions=True)
            worker.stop()
    app.cleanup_ctx.append(lifecycle)
    return app


def serve(host, port, store, config):
    web.run_app(create_app(store, config), host=host, port=port, print=None)

"""Real authorized CLI/stdin-MCP recommendation flow; only temporary local data/token."""
import json, os, subprocess, sys, tempfile
cli = sys.argv[1]
with tempfile.TemporaryDirectory(prefix="wardrobe-recommendation-cli-") as folder:
    db = os.path.join(folder, "wardrobe.sqlite")
    def trusted(operation, payload):
        response = json.loads(subprocess.check_output([cli, "--trusted-local", "--db", db, operation, "--input-stdin"], input=json.dumps(payload).encode()))
        assert "error" not in response, response
        return response["result"]
    trusted("space.default", {})
    items = [trusted("clothing.create", {"member_id":"default", "name":name, "category":category}) for name, category in [("CLI上衣A","top"),("CLI上衣B","top"),("CLI上衣C","top"),("CLI裤子","bottom"),("CLI鞋","shoes")]]
    grant = trusted("client.authorize", {"name":"临时离线验收", "mode":"limited", "operations":["recommendation.generate","recommendation.get","recommendation.list","recommendation.replace","recommendation.change","recommendation.save","wear.list","outfit.list","weather.configure","weather.get","weather.refresh"], "member_ids":[items[0]["member_id"]], "expires_at":None})
    token = grant["token"]
    def authorized(operation, payload):
        response = json.loads(subprocess.check_output([cli,"--db",db,operation,"--client-stdin"],input=json.dumps({"token":token,"input":payload}).encode()))
        assert "error" not in response, response
        return response["result"]
    def mcp(operation,payload):
        request={"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"authorization_token":token,"name":operation,"arguments":payload}}
        response=json.loads(subprocess.check_output([cli,"--db",db,"mcp"],input=(json.dumps(request)+"\n").encode()))
        assert "error" not in response and not response["result"].get("isError"),response
        return json.loads(response["result"]["content"][0]["text"])
    configured=mcp("weather.configure", {"member_id":items[0]["member_id"],"mode":"automatic","location":"Shanghai public example","latitude":31.23,"longitude":121.47,"usage":"noncommercial"})
    weather=authorized("weather.refresh", {"member_id":items[0]["member_id"]})
    assert mcp("weather.get", {"member_id":items[0]["member_id"]})["cache"]["obtained_at"]==weather["cache"]["obtained_at"]
    provider=trusted("provider.save", {"name":"network fallback","endpoint":"http://127.0.0.1:1/v1","model":"unavailable","capabilities":["text"],"authentication":"none"})
    query={"member_id":items[0]["member_id"],"operation_key":"authorized-generate","timezone":"Asia/Shanghai","strategy":"ai","provider_id":provider["id"],"weather":{"mode":"manual","temperature_c":20},"fixed_ids":[items[3]["id"]]}
    generated=authorized("recommendation.generate",query)
    assert generated["actual_count"]==3
    assert generated["generation"]["source"]=="rule_fallback"
    assert generated["generation"]["attempts"]==1
    assert mcp("recommendation.get",{"member_id":items[0]["member_id"],"id":generated["id"]})==generated
    old=next(item for item in generated["proposals"][0]["items"] if item["role"]=="top")
    replaced=mcp("recommendation.replace",{"member_id":items[0]["member_id"],"id":generated["id"],"proposal_index":0,"clothing_id":old["clothing_id"],"operation_key":"replace"})
    changed=authorized("recommendation.change",{"member_id":items[0]["member_id"],"id":replaced["id"],"proposal_index":0,"operation_key":"change"})
    assert changed["proposals"][0]["items"]!=replaced["proposals"][0]["items"]
    save={"member_id":items[0]["member_id"],"id":changed["id"],"proposal_index":0,"name":"CLI-MCP离线搭配","operation_key":"save"}
    saved=mcp("recommendation.save",save)
    assert authorized("recommendation.save",save)==saved
    assert authorized("wear.list",{"member_id":items[0]["member_id"]})["items"]==[]
    assert len(authorized("outfit.list",{"member_id":items[0]["member_id"]})["items"])==1
    assert len(mcp("recommendation.list",{"member_id":items[0]["member_id"]})["items"])==3
    print(json.dumps({"actual_weather":weather,"generated":generated,"replaced":replaced,"changed":changed,"saved":saved,"wear_count":0,"template_count":1,"authorized_operations":grant["operations"],"temporary_data_removed_on_exit":True,"credential_recorded":False},ensure_ascii=False,indent=2))

#!/usr/bin/env python3
"""故障注入 TCP 代理：向真实 WebDAV 服务器注入限流/超时/截断故障，供契约测试
验证适配器的类型化错误映射与自动重试预算。只代理文本协议所需的最小逻辑：
逐连接、逐请求处理，每个连接只处理一个请求-响应后关闭（客户端按需重连）。

用法: webdav_fault_proxy.py <listen_port> <target_port> <mode> <param> <counter_path>
  mode=rate-limit  前 <param> 个请求返回 503 + Retry-After: 0，其余透传
  mode=stall       前 <param> 个请求延迟 <stall_secs> 秒后直接断开
  mode=truncate    第 <param> 个 GET 请求：响应按 Content-Length 截断一半（服务器
                   端对象不变，模拟"传输完成但内容缺失"的完整性场景）
  mode=pass        全部透传
<counter_path> 每次请求后原子重写 {"requests":N,"faults":M}。
"""
import json
import os
import socket
import sys
import threading
import time

def main() -> None:
    listen_port, target_port, mode, param, counter_path = sys.argv[1:6]
    param = int(param)
    stall_secs = float(os.environ.get("FAULT_PROXY_STALL_SECS", "3"))
    state = {"requests": 0, "faults": 0, "gets": 0}

    def bump():
        tmp = counter_path + ".tmp"
        with open(tmp, "w") as handle:
            json.dump(state, handle)
        os.replace(tmp, counter_path)

    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", int(listen_port)))
    server.listen(16)
    print("ready", flush=True)

    def read_request(client):
        head = b""
        while b"\r\n\r\n" not in head:
            chunk = client.recv(65536)
            if not chunk:
                return None, None
            head += chunk
        headers, _, rest = head.partition(b"\r\n\r\n")
        length = 0
        for line in headers.split(b"\r\n")[1:]:
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":", 1)[1].strip())
        body = rest
        while len(body) < length:
            chunk = client.recv(65536)
            if not chunk:
                break
            body += chunk
        method = headers.split(b"\r\n")[0].split(b" ")[0].decode()
        return method, headers + b"\r\n\r\n" + body

    def forward(request):
        target = socket.create_connection(("127.0.0.1", int(target_port)), timeout=10)
        target.sendall(request)
        target.settimeout(10)
        response = b""
        while b"\r\n\r\n" not in response:
            chunk = target.recv(65536)
            if not chunk:
                return b"", b""
            response += chunk
        head, _, body = response.partition(b"\r\n\r\n")
        length = None
        for line in head.split(b"\r\n")[1:]:
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":", 1)[1].strip())
        if length is None:
            try:
                while True:
                    chunk = target.recv(65536)
                    if not chunk:
                        break
                    body += chunk
            except socket.timeout:
                pass
        else:
            while len(body) < length:
                chunk = target.recv(65536)
                if not chunk:
                    break
                body += chunk
        target.close()
        return head, body

    def handle(client):
        try:
            client.settimeout(30)
            method, request = read_request(client)
            if request is None:
                client.close()
                return
            state["requests"] += 1
            index = state["requests"]
            if mode == "rate-limit" and index <= param:
                state["faults"] += 1
                bump()
                client.sendall(
                    b"HTTP/1.1 503 Service Unavailable\r\n"
                    b"Retry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                client.close()
                return
            if mode == "stall" and index <= param:
                state["faults"] += 1
                bump()
                time.sleep(stall_secs)
                client.close()
                return
            head, body = forward(request)
            if not head:
                client.close()
                return
            truncate = mode == "truncate" and method == "GET"
            if truncate:
                state["gets"] += 1
                truncate = state["gets"] <= param
            if truncate and body:
                state["faults"] += 1
                rewritten = []
                for line in head.split(b"\r\n"):
                    if line.lower().startswith(b"content-length:"):
                        rewritten.append(
                            b"Content-Length: " + str(len(body) // 2).encode()
                        )
                    elif line.lower().startswith(b"connection:"):
                        rewritten.append(b"Connection: close")
                    else:
                        rewritten.append(line)
                if not any(l.lower().startswith(b"connection:") for l in rewritten):
                    rewritten.append(b"Connection: close")
                bump()
                client.sendall(b"\r\n".join(rewritten) + b"\r\n\r\n" + body[: len(body) // 2])
                client.close()
                return
            bump()
            client.sendall(head + b"\r\n\r\n" + body)
            client.close()
        except OSError:
            try:
                client.close()
            except OSError:
                pass

    while True:
        conn, _ = server.accept()
        threading.Thread(target=handle, args=(conn,), daemon=True).start()

if __name__ == "__main__":
    main()

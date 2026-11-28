package practice.chat;

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.URLDecoder;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Clock;
import java.util.*;
import java.util.concurrent.Executors;

public final class ChatServer {
    private static final int LIMIT = 8192;
    private final ChatService chat = new ChatService(Clock.systemUTC());
    private final Path assets;
    private ChatServer(Path assets) { this.assets = assets.toAbsolutePath().normalize(); }

    public static void main(String[] args) throws IOException {
        int port = args.length > 0 ? Integer.parseInt(args[0]) : 8787;
        Path assets = Path.of(args.length > 1 ? args[1] : "frontend/dist");
        if (!Files.isRegularFile(assets.resolve("index.html"))) throw new IOException("Build Vue before starting the server");
        ChatServer app = new ChatServer(assets);
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", port), 0);
        server.createContext("/", app::handle);
        server.setExecutor(Executors.newFixedThreadPool(6));
        server.start();
        System.out.println("松果聊天 · http://127.0.0.1:" + port);
    }

    private void handle(HttpExchange h) throws IOException {
        try {
            String path = h.getRequestURI().getPath();
            String method = h.getRequestMethod();
            if (path.equals("/api/view") && method.equals("GET")) {
                sendJson(h, 200, chat.view(form(h.getRequestURI().getRawQuery()).get("user")));
            } else if (path.equals("/api/command") && method.equals("POST")) {
                String type = h.getRequestHeaders().getFirst("Content-Type");
                if (type == null || !type.split(";", 2)[0].equalsIgnoreCase("application/x-www-form-urlencoded"))
                    throw new ChatService.Rejected(415, "FORM_REQUIRED", "需要表单格式请求");
                byte[] bytes = h.getRequestBody().readNBytes(LIMIT + 1);
                if (bytes.length > LIMIT) throw new ChatService.Rejected(413, "TOO_LARGE", "请求太长");
                var args = form(new String(bytes, StandardCharsets.UTF_8));
                sendJson(h, 200, chat.command(args.get("user"), args.getOrDefault("action", ""), args));
            } else if (path.startsWith("/api/")) {
                throw new ChatService.Rejected(404, "NO_ENDPOINT", "接口不存在");
            } else if (method.equals("GET")) {
                Path file = assets.resolve(path.equals("/") ? "index.html" : path.substring(1)).normalize();
                if (!file.startsWith(assets) || !Files.isRegularFile(file)) {
                    throw new ChatService.Rejected(404, "NOT_FOUND", "页面不存在");
                }
                String contentType = path.endsWith(".js") ? "text/javascript; charset=utf-8" :
                    path.endsWith(".css") ? "text/css; charset=utf-8" : "text/html; charset=utf-8";
                respond(h, 200, contentType, Files.readAllBytes(file));
            } else {
                throw new ChatService.Rejected(405, "METHOD_NOT_ALLOWED", "请求方法不支持");
            }
        } catch (ChatService.Rejected e) {
            sendJson(h, e.status, ChatService.obj("code", e.code, "message", e.getMessage()));
        } catch (IllegalArgumentException e) {
            sendJson(h, 400, ChatService.obj("code", "INVALID_INPUT", "请求格式不正确"));
        } catch (RuntimeException e) {
            e.printStackTrace(System.err);
            sendJson(h, 500, ChatService.obj("code", "INTERNAL", "服务暂时不可用"));
        } finally {
            h.close();
        }
    }
    private static Map<String, String> form(String raw) {
        var map = new HashMap<String, String>();
        if (raw == null || raw.isEmpty()) return map;
        for (String field : raw.split("&")) {
            String[] kv = field.split("=", 2);
            String key = URLDecoder.decode(kv[0], StandardCharsets.UTF_8);
            String value = URLDecoder.decode(kv.length == 2 ? kv[1] : "", StandardCharsets.UTF_8);
            if (map.putIfAbsent(key, value) != null) throw new IllegalArgumentException("duplicate key");
        }
        return map;
    }
    private static void sendJson(HttpExchange h, int status, Object value) throws IOException {
        respond(h, status, "application/json; charset=utf-8", json(value).getBytes(StandardCharsets.UTF_8));
    }
    private static void respond(HttpExchange h, int status, String type, byte[] bytes) throws IOException {
        h.getResponseHeaders().set("Content-Type", type);
        h.getResponseHeaders().set("Cache-Control", "no-store");
        h.getResponseHeaders().set("X-Content-Type-Options", "nosniff");
        h.sendResponseHeaders(status, bytes.length);
        h.getResponseBody().write(bytes);
    }
    private static String json(Object value) {
        if (value == null) return "null";
        if (value instanceof Boolean || value instanceof Number) return value.toString();
        if (value instanceof String s) {
            var encoded = new StringBuilder("\"");
            for (int i = 0; i < s.length(); i++) {
                char ch = s.charAt(i);
                if (ch == '"' || ch == '\\') encoded.append('\\').append(ch);
                else if (ch < 32 || Character.isSurrogate(ch)) encoded.append(String.format("\\u%04x", (int)ch));
                else encoded.append(ch);
            }
            return encoded.append('"').toString();
        }
        if (value instanceof Map<?, ?> map) {
            var out = new StringJoiner(",", "{", "}");
            map.forEach((k, v) -> out.add(json(k) + ":" + json(v)));
            return out.toString();
        }
        if (value instanceof Iterable<?> items) {
            var out = new StringJoiner(",", "[", "]");
            items.forEach(x -> out.add(json(x)));
            return out.toString();
        }
        throw new IllegalArgumentException("unsupported JSON value");
    }
}

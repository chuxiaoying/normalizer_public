package practice.chat;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.util.*;

/** In-memory training data. Every command and view uses a single consistent state. */
final class ChatService {
    record Person(String id, String name) {}
    record Invite(String from, String to, String status) {}
    enum Mode { OPEN, ADMINS_ONLY }
    record Prefs(boolean pinned, boolean muted) {}
    record Message(long id, String from, String text, Instant at, boolean recalled, String mention) {}
    static final class Rejected extends RuntimeException {
        final int status;
        final String code;
        Rejected(int status, String code, String message) { super(message); this.status = status; this.code = code; }
    }
    private static final class Room {
        final String id, title, owner;
        final Set<String> members, admins = new LinkedHashSet<>();
        final Map<String, Long> read = new HashMap<>();
        final Map<String, Prefs> prefs = new HashMap<>();
        final List<Message> messages = new ArrayList<>();
        Mode mode = Mode.OPEN;
        final boolean group;
        Room(String id, String title, String owner, boolean group, String... members) {
            this.id = id; this.title = title; this.owner = owner; this.group = group;
            this.members = new LinkedHashSet<>(List.of(members));
            if (group) admins.add(owner);
        }
    }

    private final Map<String, Person> people = new LinkedHashMap<>();
    private final Map<String, Room> rooms = new LinkedHashMap<>();
    private final Set<String> friends = new HashSet<>(), blocks = new HashSet<>();
    private final Map<String, Invite> invites = new LinkedHashMap<>();
    private final Clock clock;
    private long serial = 0;

    ChatService(Clock clock) {
        this.clock = clock;
        for (Person person : List.of(new Person("lin", "林澄"), new Person("zhou", "周屿"),
                new Person("xu", "许芒"), new Person("he", "何溪"))) people.put(person.id(), person);
        friends.add(pair("lin", "zhou"));
        Room direct = direct("lin", "zhou");
        append(direct, "zhou", "今晚八点的语音碰头还照常吗？", "");
        Room group = new Room("studio", "松果创作组", "lin", true, "lin", "zhou", "xu");
        rooms.put(group.id, group);
        append(group, "xu", "我把本周的设计稿整理好了。", "");
        append(group, "zhou", "林澄，方便的话帮我确认第二版。", "lin");
        invites.put("he>lin", new Invite("he", "lin", "PENDING"));
        // Seed history is old enough to exercise the time-window refusal.
        group.messages.add(0, new Message(0, "lin", "群公告：每周五回顾。", clock.instant().minusSeconds(600), false, ""));
    }

    synchronized Map<String, Object> view(String who) {
        person(who);
        var contacts = new ArrayList<Map<String, Object>>();
        for (Person p : people.values()) {
            if (who.equals(p.id())) continue;
            var outgoing = invites.get(who + ">" + p.id());
            var incoming = invites.get(p.id() + ">" + who);
            boolean blocked = blocks.contains(who + ">" + p.id());
            String status = blocked ? "BLOCKED" : friends.contains(pair(who, p.id())) ? "FRIEND" :
                pending(incoming) ? "INCOMING" : pending(outgoing) ? "OUTGOING" : "STRANGER";
            contacts.add(obj("id", p.id(), "name", p.name(), "status", status,
                "canInvite", status.equals("STRANGER") && !blockedEither(who, p.id())));
        }
        var result = new ArrayList<Map<String, Object>>();
        for (Room r : rooms.values()) {
            if (!r.members.contains(who)) continue;
            String peer = r.group ? "" : peer(r, who);
            var messages = new ArrayList<Map<String, Object>>();
            int unread = 0, mentions = 0;
            long seen = r.read.getOrDefault(who, 0L);
            for (Message m : r.messages) {
                if (!m.from().equals(who) && !m.recalled() && m.id() > seen) {
                    unread++;
                    if (m.mention().equals(who)) mentions++;
                }
                messages.add(obj("id", m.id(), "from", m.from(), "name", people.get(m.from()).name(),
                    "text", m.recalled() ? "" : m.text(), "at", m.at().toString(), "recalled", m.recalled(),
                    "mention", m.recalled() ? "" : m.mention(),
                    "canRecall", m.from().equals(who) && !m.recalled() && withinRecall(m)));
            }
            var pref = r.prefs.getOrDefault(who, new Prefs(false, false));
            var members = r.members.stream().map(id -> obj("id", id, "name", people.get(id).name(),
                "role", id.equals(r.owner) ? "OWNER" : r.admins.contains(id) ? "ADMIN" : "MEMBER")).toList();
            String reason = sendFailure(r, who);
            result.add(obj("id", r.id, "title", r.group ? r.title : people.get(peer).name(), "group", r.group,
                "peer", peer, "owner", r.owner, "admin", r.admins.contains(who), "mode", r.mode.name(),
                "members", members, "messages", messages, "unread", unread, "mentions", mentions,
                "pinned", pref.pinned(), "muted", pref.muted(), "canSend", reason.isEmpty(), "sendReason", reason));
        }
        return obj("people", people.values().stream().map(p -> obj("id", p.id(), "name", p.name())).toList(),
            "contacts", contacts, "rooms", result);
    }

    synchronized Map<String, Object> command(String who, String action, Map<String, String> args) {
        person(who);
        String roomId = args.getOrDefault("room", "");
        String target = args.getOrDefault("target", "");
        switch (action) {
            case "request" -> {
                other(who, target);
                if (blockedEither(who, target)) reject(403, "CONTACT_UNAVAILABLE", "暂时无法发起好友申请");
                if (friends.contains(pair(who, target))) reject(409, "ALREADY_FRIENDS", "已经是好友");
                if (pending(invites.get(who + ">" + target))) reject(409, "ALREADY_REQUESTED", "申请已在等待对方处理");
                if (pending(invites.get(target + ">" + who))) reject(409, "REVIEW_INCOMING", "对方已发来申请，请先处理");
                invites.put(who + ">" + target, new Invite(who, target, "PENDING"));
            }
            case "accept", "decline" -> {
                other(who, target);
                String key = target + ">" + who;
                if (!pending(invites.get(key))) reject(409, "NO_REQUEST", "没有待处理的申请");
                if (action.equals("accept")) {
                    if (blockedEither(who, target)) reject(403, "CONTACT_UNAVAILABLE", "无法接受此申请");
                    friends.add(pair(who, target));
                    roomId = direct(who, target).id;
                }
                invites.put(key, new Invite(target, who, action.equals("accept") ? "ACCEPTED" : "DECLINED"));
            }
            case "block", "unblock" -> {
                other(who, target);
                if (action.equals("block")) {
                    blocks.add(who + ">" + target);
                    for (String key : List.of(who + ">" + target, target + ">" + who)) {
                        Invite i = invites.get(key);
                        if (pending(i)) invites.put(key, new Invite(i.from(), i.to(), "DECLINED"));
                    }
                } else blocks.remove(who + ">" + target);
            }
            case "open" -> {
                other(who, target);
                if (!friends.contains(pair(who, target))) reject(403, "FRIENDS_ONLY", "只有好友可以发起私聊");
                roomId = direct(who, target).id;
            }
            case "send" -> {
                Room r = member(who, roomId);
                String reason = sendFailure(r, who);
                if (!reason.isEmpty()) reject(403, reason, reason.equals("ADMINS_ONLY") ? "当前仅群管理员可以发言" : "当前不能向对方发送消息");
                String value = args.getOrDefault("text", "").strip();
                if (value.isEmpty() || value.codePointCount(0, value.length()) > 300)
                    reject(400, "INVALID_MESSAGE", "消息须为 1 到 300 个字符");
                if (!target.isEmpty() && (!r.group || !r.members.contains(target) || target.equals(who)))
                    reject(400, "INVALID_MENTION", "只能提醒其他群成员");
                append(r, who, value, target);
            }
            case "recall" -> {
                Room r = member(who, roomId);
                long id;
                try { id = Long.parseLong(args.getOrDefault("id", "")); }
                catch (NumberFormatException badId) { throw new Rejected(400, "INVALID_ID", "无效消息编号"); }
                for (int index = 0; index < r.messages.size(); index++) {
                    Message m = r.messages.get(index);
                    if (m.id() != id) continue;
                    if (!who.equals(m.from())) reject(403, "NOT_AUTHOR", "只能撤回自己的消息");
                    if (m.recalled()) reject(409, "ALREADY_RECALLED", "消息已经被撤回");
                    if (!withinRecall(m)) reject(409, "RECALL_EXPIRED", "已超过两分钟撤回时限");
                    r.messages.set(index, new Message(m.id(), m.from(), "", m.at(), true, ""));
                    return obj("room", roomId, "view", view(who));
                }
                reject(404, "NO_MESSAGE", "消息不存在");
            }
            case "read" -> {
                Room r = member(who, roomId);
                r.read.put(who, r.messages.isEmpty() ? 0L : r.messages.get(r.messages.size() - 1).id());
            }
            case "pin", "mute" -> {
                Room r = member(who, roomId);
                Prefs old = r.prefs.getOrDefault(who, new Prefs(false, false));
                r.prefs.put(who, new Prefs(action.equals("pin") ? !old.pinned() : old.pinned(),
                    action.equals("mute") ? !old.muted() : old.muted()));
            }
            case "mode", "promote", "demote", "invite", "remove", "leave" -> {
                Room r = member(who, roomId);
                if (!r.group) reject(400, "GROUP_ONLY", "仅群聊可执行此操作");
                if (action.equals("mode")) {
                    if (!r.admins.contains(who)) reject(403, "ADMINS_ONLY", "只有管理员可以修改群发言权限");
                    r.mode = r.mode == Mode.OPEN ? Mode.ADMINS_ONLY : Mode.OPEN;
                } else if (action.equals("leave")) {
                    if (r.owner.equals(who)) reject(409, "OWNER_CANNOT_LEAVE", "群主不能直接退群");
                    remove(r, who);
                    roomId = "";
                } else {
                    if (!r.owner.equals(who)) reject(403, "OWNER_ONLY", "只有群主可以管理成员");
                    other(who, target);
                    if (action.equals("invite")) {
                        if (r.members.contains(target)) reject(409, "ALREADY_MEMBER", "对方已在群中");
                        if (!friends.contains(pair(who, target)) || blockedEither(who, target))
                            reject(403, "INVITE_FRIENDS_ONLY", "只能邀请可联系的好友");
                        r.members.add(target);
                        r.read.put(target, serial);
                    } else {
                        if (!r.members.contains(target)) reject(404, "NO_MEMBER", "群成员不存在");
                        if (action.equals("promote")) r.admins.add(target);
                        if (action.equals("demote")) r.admins.remove(target);
                        if (action.equals("remove")) remove(r, target);
                    }
                }
            }
            default -> reject(404, "NO_ACTION", "未知操作");
        }
        return obj("room", roomId, "view", view(who));
    }

    private Room direct(String a, String b) {
        String id = "dm:" + pair(a, b);
        return rooms.computeIfAbsent(id, k -> new Room(id, "", "", false, a, b));
    }
    private static void remove(Room room, String who) {
        room.members.remove(who); room.admins.remove(who); room.read.remove(who); room.prefs.remove(who);
    }
    private void append(Room room, String who, String content, String mention) {
        room.messages.add(new Message(++serial, who, content, clock.instant(), false, mention));
    }
    private boolean withinRecall(Message msg) {
        return Duration.between(msg.at(), clock.instant()).compareTo(Duration.ofMinutes(2)) <= 0;
    }
    private String sendFailure(Room r, String who) {
        if (r.group) return r.mode == Mode.ADMINS_ONLY && !r.admins.contains(who) ? "ADMINS_ONLY" : "";
        String peer = peer(r, who);
        return blockedEither(who, peer) || !friends.contains(pair(who, peer)) ? "CONTACT_UNAVAILABLE" : "";
    }
    private static String peer(Room r, String who) { return r.members.stream().filter(id -> !id.equals(who)).findFirst().orElseThrow(); }
    private Room member(String who, String id) {
        Room r = rooms.get(id);
        if (r == null || !r.members.contains(who)) throw new Rejected(404, "NO_ROOM", "会话不存在或不可访问");
        return r;
    }
    private void person(String id) {
        if (!people.containsKey(id)) reject(400, "INVALID_ACCOUNT", "请选择一个演示账号");
    }
    private void other(String who, String target) {
        person(target);
        if (who.equals(target)) reject(400, "SELF_TARGET", "不能对自己执行此操作");
    }
    private boolean blockedEither(String a, String b) { return blocks.contains(a + ">" + b) || blocks.contains(b + ">" + a); }
    private static boolean pending(Invite i) { return i != null && i.status().equals("PENDING"); }
    private static String pair(String a, String b) { return a.compareTo(b) < 0 ? a + ":" + b : b + ":" + a; }
    private static void reject(int status, String code, String message) { throw new Rejected(status, code, message); }
    static Map<String, Object> obj(Object... entries) {
        var map = new LinkedHashMap<String, Object>();
        for (int i = 0; i < entries.length; i += 2) map.put((String) entries[i], entries[i + 1]);
        return map;
    }
}

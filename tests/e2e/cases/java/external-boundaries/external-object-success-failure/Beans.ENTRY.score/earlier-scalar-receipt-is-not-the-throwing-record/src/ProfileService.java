import normalizer.intrinsics.N;

class Profile {
    int score;
}

class ProfileService {
    @N.External
    private final ProfileClient client;

    ProfileService(ProfileClient client) {
        this.client = client;
    }

    int score(int accountId) {
        int ignored = client.preview(accountId); Profile selected = client.fetch(accountId); return selected.score;
    }
}

class ProfileClient {
    native int preview(int accountId); native Profile fetch(int accountId) throws ProfileUnavailable;
}

class ProfileUnavailable extends RuntimeException {}

class Beans {
    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
}

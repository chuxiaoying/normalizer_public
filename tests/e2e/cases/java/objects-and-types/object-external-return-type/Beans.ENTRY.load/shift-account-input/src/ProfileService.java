import normalizer.intrinsics.N;

class Profile {
    int score;
}

class ProfileService {
    @N.External
    private final ProfileClient client;

    ProfileService(ProfileClient client) { this.client = client; }

    Profile load(int accountId) {
        return client.fetch(accountId + 1);
    }
}

class ProfileClient {
    native Profile fetch(int accountId);
}

class Beans {
    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
}

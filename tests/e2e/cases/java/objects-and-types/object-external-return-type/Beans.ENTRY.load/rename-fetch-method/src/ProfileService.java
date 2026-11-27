import normalizer.intrinsics.N;

class Profile {
    int score;
}

class ProfileService {
    @N.External
    private final ProfileClient client;

    ProfileService(ProfileClient client) { this.client = client; }

    Profile load(int accountId) {
        return client.loadProfile(accountId);
    }
}

class ProfileClient {
    native Profile loadProfile(int accountId);
}

class Beans {
    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
}

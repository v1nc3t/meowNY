package com.meowny.server.security;

import org.springframework.security.crypto.argon2.Argon2PasswordEncoder;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.util.StringUtils;

public class PepperedArgon2PasswordEncoder implements PasswordEncoder {

    private final Argon2PasswordEncoder delegate;
    private final String pepper;

    public PepperedArgon2PasswordEncoder(String pepper) {
        if (!StringUtils.hasText(pepper)) {
            throw new IllegalArgumentException("app.security.pepper must not be blank");
        }
        this.pepper = pepper;
        // 16-byte salt, 32-byte hash, 1 thread, 64MB memory, 3 iterations
        this.delegate = new Argon2PasswordEncoder(16, 32, 1, 65536, 3);
    }

    @Override
    public String encode(CharSequence rawPassword) {
        return delegate.encode(rawPassword.toString() + pepper);
    }

    @Override
    public boolean matches(CharSequence rawPassword, String encodedPassword) {
        return delegate.matches(rawPassword.toString() + pepper, encodedPassword);
    }
}

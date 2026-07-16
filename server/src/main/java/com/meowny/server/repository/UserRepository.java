package com.meowny.server.repository;

import com.meowny.commons.entity.User;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.Optional;

public interface UserRepository extends JpaRepository<User, Long> {

    Optional<User> findUserByUsername(String username);

    Optional<User> findUserByEmail(String email);

    boolean existsByUsername(String username);
    boolean existsByEmail(String email);
}

package com.meowny.server.dto.user;

import java.time.LocalDateTime;

public record UserResponse(
        Long id,
        String firstName,
        String lastName,
        String fullName,
        String email,
        String username,
        LocalDateTime createdAt
) {}

package com.meowny.server.dto.categorygroup;

import java.time.LocalDateTime;

public record CategoryGroupResponse(
        Long id,
        Long userId,
        String name,
        LocalDateTime createdAt
) {
}

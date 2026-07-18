package com.meowny.server.dto.category;

import com.meowny.server.entity.TransactionType;

import java.time.LocalDateTime;

public record CategoryResponse(
        Long id,
        Long userId,
        TransactionType type,
        String name,
        LocalDateTime createdAt,
        LocalDateTime updatedAt
) {}

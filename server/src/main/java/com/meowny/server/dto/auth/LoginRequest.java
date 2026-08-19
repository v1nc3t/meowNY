package com.meowny.server.dto.auth;

import com.fasterxml.jackson.databind.annotation.JsonDeserialize;
import com.meowny.server.config.HtmlSanitizationDeserializer;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record LoginRequest(
        @NotBlank(message = "Username is required")
        @Size(max = 50, message = "Username must be 50 characters or fewer")
        @JsonDeserialize(using = HtmlSanitizationDeserializer.class)
        String username,

        @NotBlank(message = "Password is required")
        @Size(min = 12, max = 100, message = "Password must be between 12 and 100 characters")
        String password
) {}

package com.meowny.server.service;

import com.meowny.server.dto.auth.AuthResponse;
import com.meowny.server.dto.auth.LoginRequest;
import com.meowny.server.dto.user.CreateUserRequest;
import com.meowny.server.security.JwtProvider;
import org.springframework.security.authentication.AuthenticationManager;
import org.springframework.security.authentication.UsernamePasswordAuthenticationToken;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class AuthService {

    private final UserService userService;
    private final AuthenticationManager authenticationManager;
    private final JwtProvider jwtProvider;

    public AuthService(
            UserService userService,
            AuthenticationManager authenticationManager,
            JwtProvider jwtProvider) {
        this.userService = userService;
        this.authenticationManager = authenticationManager;
        this.jwtProvider = jwtProvider;
    }

    @Transactional
    public AuthResponse register(CreateUserRequest request) {
        userService.createUser(request);
        return login(new LoginRequest(request.username(), request.password()));
    }

    public AuthResponse login(LoginRequest request) {
        authenticationManager.authenticate(
                new UsernamePasswordAuthenticationToken(request.username(), request.password())
        );
        return new AuthResponse(jwtProvider.generateToken(request.username()), "Bearer");
    }
}

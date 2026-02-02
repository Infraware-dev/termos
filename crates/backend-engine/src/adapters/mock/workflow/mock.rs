use super::*;

pub fn mock_workflow() -> Workflow {
    let mut playbooks = HashMap::new();

    let ls_playbook = Playbook {
        name: "List Files".to_string(),
        intents: vec!["how to list files".to_string(), "show me files".to_string(), "what files are here".to_string()],
        phases: vec![
            Phase {
                phase: 1,
                name: "Provide ls command".to_string(),
                description: "Respond with the appropriate ls command to list files.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 1,
                        action: "Listing files".to_string(),
                        command: "ls -la".to_string(),
                        output: "total 12\ndrwxr-xr-x  3 user user 4096 Jun 10 12:00 .\ndrwxr-xr-x 10 user user 4096 Jun 10 11:00 ..\n-rw-r--r--  1 user user   23 Jun 10 12:00 file.txt\n".to_string(),
                        analysis: "Listed files including hidden ones in current directory.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: Some("Listed files successfully".to_string()),
            }
        ]
    };

    playbooks.insert("list-files".to_string(), ls_playbook);

    // Docker playbook
    let docker_playbook = Playbook {
        name: "Docker Container Management".to_string(),
        intents: vec!["show me running containers".to_string(), "what containers are running".to_string(), "how to list containers".to_string()],
        phases: vec![
            Phase {
                phase: 1,
                name: "Check Running Containers".to_string(),
                description: "List all running Docker containers to assess current state.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 1,
                        action: "Listing running containers".to_string(),
                        command: "docker ps".to_string(),
                        output: "CONTAINER ID   IMAGE          COMMAND                  CREATED        STATUS        PORTS                    NAMES\na1b2c3d4e5f6   nginx:latest   \"/docker-entrypoint.…\"   2 hours ago    Up 2 hours    0.0.0.0:80->80/tcp       web-server\nb2c3d4e5f6a7   redis:alpine   \"docker-entrypoint.s…\"   3 hours ago    Up 3 hours    0.0.0.0:6379->6379/tcp   cache\n".to_string(),
                        analysis: "Found 2 running containers: nginx web server on port 80 and Redis cache on port 6379.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: None,
            },
            Phase {
                phase: 2,
                name: "Inspect Container Images".to_string(),
                description: "List available Docker images on the system.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 2,
                        action: "Listing Docker images".to_string(),
                        command: "docker images".to_string(),
                        output: "REPOSITORY   TAG       IMAGE ID       CREATED        SIZE\nnginx        latest    a6bd71f48f68   2 weeks ago    187MB\nredis        alpine    3900abf41552   3 weeks ago    40MB\npostgres     15        ceccf204404e   1 month ago    379MB\n".to_string(),
                        analysis: "Three images available: nginx, redis (alpine variant), and postgres 15.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: None,
            },
            Phase {
                phase: 3,
                name: "Check Container Logs".to_string(),
                description: "Review recent logs from the web server container.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 3,
                        action: "Fetching container logs".to_string(),
                        command: "docker logs --tail 10 web-server".to_string(),
                        output: "2026/01/30 10:15:32 [notice] 1#1: nginx/1.25.3\n2026/01/30 10:15:32 [notice] 1#1: built by gcc 12.2.0\n2026/01/30 10:15:32 [notice] 1#1: OS: Linux 5.15.0-91-generic\n2026/01/30 10:15:32 [notice] 1#1: start worker processes\n172.17.0.1 - - [30/Jan/2026:10:20:15 +0000] \"GET / HTTP/1.1\" 200 615\n172.17.0.1 - - [30/Jan/2026:10:21:03 +0000] \"GET /api/health HTTP/1.1\" 200 2\n".to_string(),
                        analysis: "Nginx started successfully. Recent requests show healthy traffic including health check endpoint.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: None,
            },
        ],
    };
    playbooks.insert("docker".to_string(), docker_playbook);

    // Kubernetes playbook
    let k8s_playbook = Playbook {
        name: "Kubernetes Cluster Inspection".to_string(),
        intents: vec!["how to list pods".to_string(), "show me the pods".to_string(), "what pods are running".to_string()],
        phases: vec![
            Phase {
                phase: 1,
                name: "Check Pod Status".to_string(),
                description: "List all pods in the default namespace to verify cluster health.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 1,
                        action: "Listing pods in default namespace".to_string(),
                        command: "kubectl get pods".to_string(),
                        output: "NAME                              READY   STATUS    RESTARTS   AGE\napi-deployment-7d4f8b9c6-x2k9p    1/1     Running   0          2d\napi-deployment-7d4f8b9c6-m3n7q    1/1     Running   0          2d\ndb-statefulset-0                  1/1     Running   0          5d\nredis-cache-5f6d7e8c9-abc12       1/1     Running   1          3d\n".to_string(),
                        analysis: "4 pods running: 2 API replicas, 1 database statefulset, 1 Redis cache. All healthy with minimal restarts.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: Some("All pods are running healthy".to_string()),
            },
            Phase {
                phase: 2,
                name: "Check Services".to_string(),
                description: "List Kubernetes services to verify networking configuration.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 2,
                        action: "Listing services".to_string(),
                        command: "kubectl get services".to_string(),
                        output: "NAME         TYPE           CLUSTER-IP       EXTERNAL-IP      PORT(S)        AGE\nkubernetes   ClusterIP      10.96.0.1        <none>           443/TCP        30d\napi-svc      LoadBalancer   10.96.128.45     34.102.136.208   80:31234/TCP   2d\ndb-svc       ClusterIP      10.96.200.12     <none>           5432/TCP       5d\nredis-svc    ClusterIP      10.96.180.33     <none>           6379/TCP       3d\n".to_string(),
                        analysis: "Services configured correctly. API exposed via LoadBalancer with external IP. Database and Redis internal only.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: None,
            },
            Phase {
                phase: 3,
                name: "Describe Problem Pod".to_string(),
                description: "Get detailed information about a specific pod for troubleshooting.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 3,
                        action: "Describing pod details".to_string(),
                        command: "kubectl describe pod api-deployment-7d4f8b9c6-x2k9p".to_string(),
                        output: "Name:             api-deployment-7d4f8b9c6-x2k9p\nNamespace:        default\nNode:             gke-cluster-node-pool-a1b2c3d4/10.128.0.45\nStatus:           Running\nIP:               10.244.1.15\nContainers:\n  api:\n    Image:          myapp/api:v2.1.0\n    State:          Running\n    Ready:          True\n    Restart Count:  0\n    Liveness:       http-get http://:8080/health delay=10s\n    Readiness:      http-get http://:8080/ready delay=5s\nEvents:\n  Normal  Scheduled  2d   default-scheduler  Successfully assigned\n  Normal  Pulled     2d   kubelet            Container image pulled\n  Normal  Started    2d   kubelet            Started container api\n".to_string(),
                        analysis: "Pod healthy on node gke-cluster-node-pool. Liveness and readiness probes configured. No recent issues in events.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: Some("Pod is healthy with no issues detected.".to_string()),
            },
        ],
    };
    playbooks.insert("kubernetes".to_string(), k8s_playbook);

    // Git playbook
    let git_playbook = Playbook {
        name: "Git Repository Operations".to_string(),
        intents: vec!["how to commit all changes".to_string(), "please push changes".to_string(), "help me push to remote".to_string()],
        phases: vec![
            Phase {
                phase: 1,
                name: "Check Repository Status".to_string(),
                description: "Inspect the current state of the Git repository.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 1,
                        action: "Checking git status".to_string(),
                        command: "git status".to_string(),
                        output: "On branch feature/user-auth\nYour branch is up to date with 'origin/feature/user-auth'.\n\nChanges not staged for commit:\n  (use \"git add <file>...\" to update what will be committed)\n        modified:   src/auth/handler.rs\n        modified:   src/auth/middleware.rs\n\nUntracked files:\n  (use \"git add <file>...\" to include in what will be committed)\n        src/auth/tests/\n\nno changes added to commit (use \"git add\" and/or \"git commit -a\")\n".to_string(),
                        analysis: "On feature branch with 2 modified files and new test directory. Changes not yet staged.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: None,
            },
            Phase {
                phase: 2,
                name: "Stage and Commit Changes".to_string(),
                description: "Add modified files to staging and create a commit.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 2,
                        action: "Staging all changes".to_string(),
                        command: "git add .".to_string(),
                        output: "".to_string(),
                        analysis: "All changes staged for commit including new test files.".to_string(),
                    },
                    Step {
                        step: 3,
                        action: "Creating commit".to_string(),
                        command: "git commit -m \"feat: implement JWT authentication middleware\"".to_string(),
                        output: "[feature/user-auth a1b2c3d] feat: implement JWT authentication middleware\n 3 files changed, 245 insertions(+), 12 deletions(-)\n create mode 100644 src/auth/tests/middleware_test.rs\n".to_string(),
                        analysis: "Commit created successfully with 245 lines added across 3 files.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: None,
            },
            Phase {
                phase: 3,
                name: "Push to Remote".to_string(),
                description: "Push committed changes to the remote repository.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 4,
                        action: "Pushing to remote".to_string(),
                        command: "git push".to_string(),
                        output: "Enumerating objects: 12, done.\nCounting objects: 100% (12/12), done.\nDelta compression using up to 8 threads\nCompressing objects: 100% (8/8), done.\nWriting objects: 100% (8/8), 2.34 KiB | 2.34 MiB/s, done.\nTotal 8 (delta 5), reused 0 (delta 0), pack-reused 0\nremote: Resolving deltas: 100% (5/5), completed with 3 local objects.\nTo github.com:myorg/myrepo.git\n   d4e5f6a..a1b2c3d  feature/user-auth -> feature/user-auth\n".to_string(),
                        analysis: "Changes pushed successfully to origin. Remote branch updated.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: None,
            },
            Phase {
                phase: 4,
                name: "View Recent History".to_string(),
                description: "Display recent commit history for review.".to_string(),
                steps: Some(vec![
                    Step {
                        step: 5,
                        action: "Viewing commit log".to_string(),
                        command: "git log --oneline -5".to_string(),
                        output: "a1b2c3d (HEAD -> feature/user-auth, origin/feature/user-auth) feat: implement JWT authentication middleware\nd4e5f6a fix: resolve token expiration edge case\nc3b2a1f feat: add user login endpoint\nb2a1c3d refactor: extract auth utilities\na1c2b3e chore: update dependencies\n".to_string(),
                        analysis: "Recent history shows clean feature development with proper commit conventions.".to_string(),
                    }
                ]),
                duration_minutes: None,
                root_cause: None,
                verification_summary: None,
                conclusion: Some("View Recent History".to_string()),
            },
        ],
    };
    playbooks.insert("git".to_string(), git_playbook);

    Workflow {
        playbooks,
        run_commands: false,
    }
}

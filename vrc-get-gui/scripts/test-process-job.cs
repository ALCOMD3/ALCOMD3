using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

namespace Alcomd3.E2E
{
    public sealed class TestProcessJob : IDisposable
    {
        private const uint CreateSuspended = 0x00000004;
        private const uint JobObjectLimitKillOnJobClose = 0x00002000;
        private const int JobObjectExtendedLimitInformationClass = 9;
        private const uint WaitObject0 = 0x00000000;
        private const uint WaitTimeout = 0x00000102;
        private const uint WaitFailed = 0xFFFFFFFF;
        private const uint StillActive = 259;
        private IntPtr jobHandle;
        private bool started;

        public TestProcessJob()
        {
            jobHandle = CreateJobObject(IntPtr.Zero, null);
            if (jobHandle == IntPtr.Zero)
            {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Failed to create desktop E2E job object.");
            }

            var information = new JobObjectExtendedLimitInformation();
            information.BasicLimitInformation.LimitFlags = JobObjectLimitKillOnJobClose;
            var informationLength = Marshal.SizeOf(typeof(JobObjectExtendedLimitInformation));
            var informationPointer = Marshal.AllocHGlobal(informationLength);
            try
            {
                Marshal.StructureToPtr(information, informationPointer, false);
                if (!SetInformationJobObject(
                    jobHandle,
                    JobObjectExtendedLimitInformationClass,
                    informationPointer,
                    (uint)informationLength))
                {
                    throw new Win32Exception(
                        Marshal.GetLastWin32Error(),
                        "Failed to configure desktop E2E job object."
                    );
                }
            }
            catch
            {
                CloseHandle(jobHandle);
                jobHandle = IntPtr.Zero;
                throw;
            }
            finally
            {
                Marshal.FreeHGlobal(informationPointer);
            }
        }

        public JobProcess Start(string applicationPath, string[] arguments, string workingDirectory)
        {
            if (jobHandle == IntPtr.Zero)
            {
                throw new ObjectDisposedException("TestProcessJob");
            }
            if (started)
            {
                throw new InvalidOperationException("The desktop E2E job already has a root process.");
            }

            var startupInformation = new StartupInformation();
            startupInformation.Size = Marshal.SizeOf(typeof(StartupInformation));
            ProcessInformation processInformation;
            var commandLine = new StringBuilder(BuildCommandLine(applicationPath, arguments));
            if (!CreateProcess(
                applicationPath,
                commandLine,
                IntPtr.Zero,
                IntPtr.Zero,
                false,
                CreateSuspended,
                IntPtr.Zero,
                workingDirectory,
                ref startupInformation,
                out processInformation))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Failed to start WebdriverIO suspended.");
            }

            try
            {
                if (!AssignProcessToJobObject(jobHandle, processInformation.Process))
                {
                    throw new Win32Exception(
                        Marshal.GetLastWin32Error(),
                        "Failed to assign WebdriverIO to the desktop E2E job object."
                    );
                }
                if (ResumeThread(processInformation.Thread) == uint.MaxValue)
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Failed to resume WebdriverIO.");
                }

                var process = new JobProcess(processInformation.Process);
                processInformation.Process = IntPtr.Zero;
                started = true;
                return process;
            }
            catch
            {
                TerminateProcess(processInformation.Process, 1);
                throw;
            }
            finally
            {
                CloseHandle(processInformation.Thread);
                if (processInformation.Process != IntPtr.Zero)
                {
                    CloseHandle(processInformation.Process);
                }
            }
        }

        public sealed class JobProcess : IDisposable
        {
            private IntPtr processHandle;

            internal JobProcess(IntPtr processHandle)
            {
                this.processHandle = processHandle;
            }

            public bool HasExited
            {
                get
                {
                    var waitResult = WaitForSingleObject(GetHandle(), 0);
                    if (waitResult == WaitObject0)
                    {
                        return true;
                    }
                    if (waitResult == WaitTimeout)
                    {
                        return false;
                    }
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Failed to inspect WebdriverIO state.");
                }
            }

            public int ExitCode
            {
                get
                {
                    uint exitCode;
                    if (!GetExitCodeProcess(GetHandle(), out exitCode))
                    {
                        throw new Win32Exception(
                            Marshal.GetLastWin32Error(),
                            "Failed to read WebdriverIO exit code."
                        );
                    }
                    if (exitCode == StillActive)
                    {
                        throw new InvalidOperationException("WebdriverIO has not exited.");
                    }
                    return unchecked((int)exitCode);
                }
            }

            public bool WaitForExit(int milliseconds)
            {
                if (milliseconds < 0)
                {
                    throw new ArgumentOutOfRangeException("milliseconds");
                }

                var waitResult = WaitForSingleObject(GetHandle(), (uint)milliseconds);
                if (waitResult == WaitObject0)
                {
                    return true;
                }
                if (waitResult == WaitTimeout)
                {
                    return false;
                }
                if (waitResult == WaitFailed)
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Failed to wait for WebdriverIO.");
                }
                throw new InvalidOperationException("Unexpected WebdriverIO wait result.");
            }

            public void Dispose()
            {
                if (processHandle != IntPtr.Zero)
                {
                    CloseHandle(processHandle);
                    processHandle = IntPtr.Zero;
                }
                GC.SuppressFinalize(this);
            }

            ~JobProcess()
            {
                Dispose();
            }

            private IntPtr GetHandle()
            {
                if (processHandle == IntPtr.Zero)
                {
                    throw new ObjectDisposedException("JobProcess");
                }
                return processHandle;
            }
        }

        public void Dispose()
        {
            if (jobHandle != IntPtr.Zero)
            {
                CloseHandle(jobHandle);
                jobHandle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~TestProcessJob()
        {
            Dispose();
        }

        private static string BuildCommandLine(string applicationPath, string[] arguments)
        {
            var commandLine = new StringBuilder(QuoteArgument(applicationPath));
            foreach (var argument in arguments)
            {
                commandLine.Append(' ');
                commandLine.Append(QuoteArgument(argument));
            }
            return commandLine.ToString();
        }

        private static string QuoteArgument(string argument)
        {
            if (argument.Length > 0 && argument.IndexOfAny(new[] { ' ', '\t', '\n', '\v', '"' }) < 0)
            {
                return argument;
            }

            var quoted = new StringBuilder("\"");
            var backslashes = 0;
            foreach (var character in argument)
            {
                if (character == '\\')
                {
                    backslashes++;
                    continue;
                }
                if (character == '"')
                {
                    quoted.Append('\\', (backslashes * 2) + 1);
                    quoted.Append(character);
                    backslashes = 0;
                    continue;
                }

                quoted.Append('\\', backslashes);
                backslashes = 0;
                quoted.Append(character);
            }
            quoted.Append('\\', backslashes * 2);
            quoted.Append('"');
            return quoted.ToString();
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateJobObject(IntPtr jobAttributes, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetInformationJobObject(
            IntPtr job,
            int informationClass,
            IntPtr jobObjectInformation,
            uint jobObjectInformationLength
        );

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool CreateProcess(
            string applicationName,
            StringBuilder commandLine,
            IntPtr processAttributes,
            IntPtr threadAttributes,
            bool inheritHandles,
            uint creationFlags,
            IntPtr environment,
            string currentDirectory,
            ref StartupInformation startupInformation,
            out ProcessInformation processInformation
        );

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint ResumeThread(IntPtr thread);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool TerminateProcess(IntPtr process, uint exitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr handle);

        [StructLayout(LayoutKind.Sequential)]
        private struct StartupInformation
        {
            internal int Size;
            internal string Reserved;
            internal string Desktop;
            internal string Title;
            internal uint X;
            internal uint Y;
            internal uint XSize;
            internal uint YSize;
            internal uint XCountChars;
            internal uint YCountChars;
            internal uint FillAttribute;
            internal uint Flags;
            internal ushort ShowWindow;
            internal ushort Reserved2;
            internal IntPtr ReservedPointer;
            internal IntPtr StandardInput;
            internal IntPtr StandardOutput;
            internal IntPtr StandardError;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct ProcessInformation
        {
            internal IntPtr Process;
            internal IntPtr Thread;
            internal uint ProcessId;
            internal uint ThreadId;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JobObjectBasicLimitInformation
        {
            internal long PerProcessUserTimeLimit;
            internal long PerJobUserTimeLimit;
            internal uint LimitFlags;
            internal UIntPtr MinimumWorkingSetSize;
            internal UIntPtr MaximumWorkingSetSize;
            internal uint ActiveProcessLimit;
            internal UIntPtr Affinity;
            internal uint PriorityClass;
            internal uint SchedulingClass;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct IoCounters
        {
            internal ulong ReadOperationCount;
            internal ulong WriteOperationCount;
            internal ulong OtherOperationCount;
            internal ulong ReadTransferCount;
            internal ulong WriteTransferCount;
            internal ulong OtherTransferCount;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JobObjectExtendedLimitInformation
        {
            internal JobObjectBasicLimitInformation BasicLimitInformation;
            internal IoCounters IoInfo;
            internal UIntPtr ProcessMemoryLimit;
            internal UIntPtr JobMemoryLimit;
            internal UIntPtr PeakProcessMemoryUsed;
            internal UIntPtr PeakJobMemoryUsed;
        }
    }
}

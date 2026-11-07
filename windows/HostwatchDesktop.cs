// Native .NET host for the embedded WPF dashboard. No terminal or execution-policy changes.
using System;
using System.IO;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using System.Threading;
using System.Windows.Forms;

internal static class HostwatchDesktop
{
    [STAThread]
    private static int Main()
    {
        try
        {
            string backend = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "hostwatch.exe");
            if (!File.Exists(backend)) throw new FileNotFoundException("Please keep Hostwatch-Windows.exe and hostwatch.exe together.");
            string source;
            using (Stream stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("dashboard.ps1"))
            using (StreamReader reader = new StreamReader(stream, System.Text.Encoding.UTF8))
                source = reader.ReadToEnd();
            using (Runspace runspace = RunspaceFactory.CreateRunspace())
            {
                runspace.ApartmentState = ApartmentState.STA;
                runspace.ThreadOptions = PSThreadOptions.UseCurrentThread;
                runspace.Open();
                using (PowerShell shell = PowerShell.Create())
                {
                    shell.Runspace = runspace;
                    shell.AddScript(source).AddParameter("Backend", backend).Invoke();
                    if (shell.InvocationStateInfo.State == PSInvocationState.Failed)
                        throw shell.InvocationStateInfo.Reason;
                }
            }
            return 0;
        }
        catch (Exception error)
        {
            MessageBox.Show(error.Message, "Hostwatch", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return 1;
        }
    }
}
